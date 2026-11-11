//! Signal graph plus a double-buffered op batch.
//!
//! Scripts own a `Turn`; the host owns `Shell`. Bindings live in the host.
//! This type records which binding read which signal, marks those bindings
//! dirty on write, and coalesces the ops pushed while rerunning them. `commit`
//! only swaps buffers and encodes a batch; it never mutates the tree.

use std::collections::HashSet;

use crate::gesture::GestureKind;
use crate::ir::NodeDesc;
use crate::ops::{encode, Op, Value};

/// Signal values stay in the script host. The graph only records which
/// binding read which signal and which bindings a write dirtied. Equality
/// is the host's (`===` for JS): an array or object is a reference, never
/// a copy stored here.

pub struct Turn {
    /// Index 0 is unused so a zero id is never a live signal.
    subs: Vec<HashSet<u32>>,
    reads: Vec<Vec<u32>>,
    tracking: Option<u32>,
    dirty: Vec<u32>,
    dirty_set: HashSet<u32>,
    bufs: [Vec<Op>; 2],
    write: usize,
    seq: u32,
    next_handle: u32,
    next_gesture: u32,
}

impl Turn {
    pub fn new() -> Self {
        Self {
            subs: vec![HashSet::new()],
            reads: vec![Vec::new()],
            tracking: None,
            dirty: Vec::new(),
            dirty_set: HashSet::new(),
            bufs: [Vec::new(), Vec::new()],
            write: 0,
            seq: 0,
            next_handle: 2,
            next_gesture: 1,
        }
    }

    pub fn next_handle(&self) -> u32 {
        self.next_handle
    }

    /// A new signal id. The host stores the value.
    pub fn alloc_signal(&mut self) -> u32 {
        let id = self.subs.len() as u32;
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

    /// Subscribe the tracking binding to `id`. Does not return the value.
    pub fn read(&mut self, id: u32) -> Result<(), String> {
        self.live(id)?;
        self.record_read(id);
        Ok(())
    }

    /// The host already decided the value changed. Mark subscribers dirty.
    pub fn notify(&mut self, id: u32) -> Result<(), String> {
        self.live(id)?;
        for binding in &self.subs[id as usize] {
            if self.dirty_set.insert(*binding) {
                self.dirty.push(*binding);
            }
        }
        Ok(())
    }

    fn live(&self, id: u32) -> Result<(), String> {
        if id == 0 || self.subs.get(id as usize).is_none() {
            return Err(format!("unknown signal {id}"));
        }
        Ok(())
    }

    fn record_read(&mut self, id: u32) {
        if let Some(binding) = self.tracking {
            self.subs[id as usize].insert(binding);
            self.reads[binding as usize].push(id);
        }
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

    /// Allocate a gesture id and queue its declaration under `target`.
    pub fn spawn_gesture(
        &mut self,
        target: u32,
        kind: GestureKind,
        index: u32,
    ) -> Result<u32, String> {
        let gesture = self.next_gesture;
        self.next_gesture = self
            .next_gesture
            .checked_add(1)
            .ok_or_else(|| "gesture space exhausted".to_string())?;
        self.push(Op::SpawnGesture {
            gesture,
            target,
            kind: kind as u32,
            index,
        });
        Ok(gesture)
    }

    pub fn set_gesture_prop(&mut self, gesture: u32, key: impl Into<String>, value: Value) {
        self.push(Op::SetGestureProp {
            gesture,
            key: key.into(),
            value,
        });
    }

    pub fn bind_gesture_event(&mut self, gesture: u32, slot: u8, handler: u32) {
        self.push(Op::BindGestureEvent {
            gesture,
            slot,
            handler,
        });
    }

    pub fn move_gesture(&mut self, gesture: u32, target: u32, index: u32) {
        self.push(Op::MoveGesture {
            gesture,
            target,
            index,
        });
    }

    pub fn dispose_gesture(&mut self, gesture: u32) {
        self.push(Op::DisposeGesture { gesture });
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
        Op::SetGestureProp {
            gesture,
            key,
            value,
        } => {
            for item in buf.iter_mut().rev() {
                match item {
                    Op::SetGestureProp {
                        gesture: existing,
                        key: existing_key,
                        value: slot,
                    } if existing == gesture && existing_key == key => {
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
