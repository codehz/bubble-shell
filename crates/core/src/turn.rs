//! Signal graph plus a double-buffered op batch.
//!
//! Bindings live in the host. This type only records which binding read which
//! signal, marks those bindings dirty on write, and coalesces the ops the host
//! pushes while rerunning them. `commit` swaps buffers and applies one batch.

use std::collections::HashSet;

use crate::ops::{encode, Op};
use crate::shell::Shell;

pub struct Turn {
    shell: Shell,
    values: Vec<f64>,
    subs: Vec<HashSet<u32>>,
    reads: Vec<Vec<u32>>,
    tracking: Option<u32>,
    dirty: Vec<u32>,
    dirty_set: HashSet<u32>,
    bufs: [Vec<Op>; 2],
    write: usize,
    seq: u32,
}

impl Turn {
    pub fn new(shell: Shell) -> Self {
        Self {
            shell,
            values: vec![0.0],
            subs: vec![HashSet::new()],
            reads: vec![Vec::new()],
            tracking: None,
            dirty: Vec::new(),
            dirty_set: HashSet::new(),
            bufs: [Vec::new(), Vec::new()],
            write: 0,
            seq: 0,
        }
    }

    pub fn shell(&self) -> &Shell {
        &self.shell
    }

    pub fn shell_mut(&mut self) -> &mut Shell {
        &mut self.shell
    }

    pub fn signal(&mut self, initial: f64) -> u32 {
        let id = self.values.len() as u32;
        self.values.push(initial);
        self.subs.push(HashSet::new());
        id
    }

    pub fn track(&mut self, binding: u32) {
        if self.reads.len() <= binding as usize {
            self.reads.resize(binding as usize + 1, Vec::new());
        }
        for signal in self.reads[binding as usize].drain(..) {
            self.subs[signal as usize].remove(&binding);
        }
        self.tracking = Some(binding);
    }

    pub fn untrack(&mut self) {
        self.tracking = None;
    }

    pub fn get(&mut self, id: u32) -> Result<f64, String> {
        let value = *self
            .values
            .get(id as usize)
            .ok_or_else(|| format!("unknown signal {id}"))?;
        if let Some(binding) = self.tracking {
            self.subs[id as usize].insert(binding);
            self.reads[binding as usize].push(id);
        }
        Ok(value)
    }

    pub fn set(&mut self, id: u32, value: f64) -> Result<(), String> {
        let slot = self
            .values
            .get_mut(id as usize)
            .ok_or_else(|| format!("unknown signal {id}"))?;
        if *slot == value {
            return Ok(());
        }
        *slot = value;
        for binding in &self.subs[id as usize] {
            if self.dirty_set.insert(*binding) {
                self.dirty.push(*binding);
            }
        }
        Ok(())
    }

    pub fn take_dirty(&mut self) -> Vec<u32> {
        self.dirty_set.clear();
        std::mem::take(&mut self.dirty)
    }

    pub fn push(&mut self, op: Op) {
        coalesce(&mut self.bufs[self.write], op);
    }

    /// Swap the write buffer out, apply it, and return the encoded batch.
    pub fn commit(&mut self) -> Result<Vec<u8>, String> {
        let read = self.write;
        self.write ^= 1;
        self.bufs[self.write].clear();
        self.seq += 1;
        let bytes = encode(self.seq, &self.bufs[read]);
        self.shell.apply_ops(&bytes)?;
        self.bufs[read].clear();
        Ok(bytes)
    }
}

fn coalesce(buf: &mut Vec<Op>, op: Op) {
    match &op {
        Op::SetText { handle, text } => {
            for item in buf.iter_mut().rev() {
                match item {
                    Op::SetText { handle: existing, text: slot } if existing == handle => {
                        *slot = text.clone();
                        return;
                    }
                    Op::SetPropColor { .. } => {}
                    _ => break,
                }
            }
        }
        Op::SetPropColor { handle, key, rgba } => {
            for item in buf.iter_mut().rev() {
                match item {
                    Op::SetPropColor {
                        handle: existing,
                        key: existing_key,
                        rgba: slot,
                    } if existing == handle && existing_key == key => {
                        *slot = *rgba;
                        return;
                    }
                    Op::SetText { .. } | Op::SetPropColor { .. } => {}
                    _ => break,
                }
            }
        }
        _ => {}
    }
    buf.push(op);
}
