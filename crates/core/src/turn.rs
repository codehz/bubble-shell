//! Signal graph plus a double-buffered op batch.
//!
//! Scripts own a `Turn`; the host owns `Shell`. Bindings live in the host.
//! This type records which binding read which signal, marks those bindings
//! dirty on write, and coalesces the ops pushed while rerunning them. `commit`
//! only swaps buffers and encodes a batch; it never mutates the tree.

use std::collections::HashSet;

use crate::ir::NodeDesc;
use crate::ops::{encode, Op, Value};

pub struct Turn {
    values: Vec<f64>,
    subs: Vec<HashSet<u32>>,
    reads: Vec<Vec<u32>>,
    tracking: Option<u32>,
    dirty: Vec<u32>,
    dirty_set: HashSet<u32>,
    bufs: [Vec<Op>; 2],
    write: usize,
    seq: u32,
    next_handle: u32,
}

impl Turn {
    pub fn new() -> Self {
        Self {
            values: vec![0.0],
            subs: vec![HashSet::new()],
            reads: vec![Vec::new()],
            tracking: None,
            dirty: Vec::new(),
            dirty_set: HashSet::new(),
            bufs: [Vec::new(), Vec::new()],
            write: 0,
            seq: 0,
            next_handle: 2,
        }
    }

    pub fn next_handle(&self) -> u32 {
        self.next_handle
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

    /// Allocate a handle and queue the node for insertion. The op is applied
    /// by the host, so callers may build children under the returned handle
    /// before the parent exists in the tree.
    pub fn spawn(&mut self, mut desc: NodeDesc) -> Result<u32, String> {
        let handle = self.allocate_handle()?;
        desc.id = handle;
        self.push(Op::Spawn { desc });
        Ok(handle)
    }

    /// Spawn at a final child index; used by fragment anchors.
    pub fn spawn_at(&mut self, mut desc: NodeDesc, index: u32) -> Result<u32, String> {
        let handle = self.allocate_handle()?;
        desc.id = handle;
        self.push(Op::SpawnAt { desc, index });
        Ok(handle)
    }

    pub fn dispose(&mut self, handle: u32) {
        self.push(Op::DisposeNode { handle });
    }

    pub fn move_node(&mut self, handle: u32, parent: u32, index: u32) {
        self.push(Op::MoveNode {
            handle,
            parent,
            index,
        });
    }

    fn allocate_handle(&mut self) -> Result<u32, String> {
        let handle = self.next_handle;
        self.next_handle = self
            .next_handle
            .checked_add(1)
            .ok_or_else(|| "handle space exhausted".to_string())?;
        Ok(handle)
    }

    pub fn bind_event(&mut self, handle: u32, handler: u32) {
        self.push(Op::BindEvent { handle, handler });
    }

    pub fn set_prop(&mut self, handle: u32, key: impl Into<String>, value: Value) {
        self.push(Op::SetProp {
            handle,
            key: key.into(),
            value,
        });
    }

    pub fn push(&mut self, op: Op) {
        coalesce(&mut self.bufs[self.write], op);
    }

    /// Swap the write buffer out and encode it. The caller applies the batch
    /// to the tree on the host/frame thread.
    pub fn commit(&mut self) -> Result<Vec<u8>, String> {
        let read = self.write;
        self.write ^= 1;
        self.bufs[self.write].clear();
        self.seq += 1;
        let bytes = encode(self.seq, &self.bufs[read]);
        self.bufs[read].clear();
        Ok(bytes)
    }
}

fn coalesce(buf: &mut Vec<Op>, op: Op) {
    match &op {
        Op::SetText { handle, text } => {
            for item in buf.iter_mut().rev() {
                match item {
                    Op::SetText {
                        handle: existing,
                        text: slot,
                    } if existing == handle => {
                        *slot = text.clone();
                        return;
                    }
                    Op::SetPropColor { .. } | Op::SetProp { .. } => {}
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
                    Op::SetText { .. } | Op::SetPropColor { .. } | Op::SetProp { .. } => {}
                    _ => break,
                }
            }
        }
        Op::SetProp { handle, key, value } => {
            for item in buf.iter_mut().rev() {
                match item {
                    Op::SetProp {
                        handle: existing,
                        key: existing_key,
                        value: slot,
                    } if existing == handle && existing_key == key => {
                        *slot = value.clone();
                        return;
                    }
                    Op::SetText { .. } | Op::SetPropColor { .. } | Op::SetProp { .. } => {}
                    _ => break,
                }
            }
        }
        _ => {}
    }
    buf.push(op);
}
