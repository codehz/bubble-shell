//! Signal graph plus a double-buffered op batch.
//!
//! Scripts own a `Turn`; the host owns `Shell`. Bindings live in the host.
//! This type records which binding read which signal, marks those bindings
//! dirty on write, and coalesces the ops pushed while rerunning them. `commit`
//! only swaps buffers and encodes a batch; it never mutates the tree.

use crate::reactive::{self, Graph, Kind, Sched};

use crate::gesture::GestureKind;
use crate::ir::NodeDesc;
use crate::ops::{encode, Op, Value};

pub struct Turn {
    /// Values stay in the script host. [`Graph`] schedules memos and effects.
    graph: Graph,
    bufs: [Vec<Op>; 2],
    write: usize,
    seq: u32,
    next_handle: u32,
    next_gesture: u32,
}

impl Turn {
    pub fn new() -> Self {
        Self {
            graph: Graph::new(),
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

    pub fn alloc_signal(&mut self) -> u32 {
        self.graph.alloc_signal()
    }

    pub fn alloc_computation(&mut self, kind: Kind) -> Result<u32, String> {
        self.graph.alloc_computation(kind)
    }

    pub fn mount_computation(&mut self, id: u32, host: &mut dyn Sched) -> Result<(), String> {
        let graph = &mut self.graph as *mut Graph;
        reactive::mount(graph, id, host)
    }

    pub fn read(&mut self, id: u32, host: &mut dyn Sched) -> Result<(), String> {
        let graph = &mut self.graph as *mut Graph;
        reactive::read(graph, id, host)
    }

    pub fn notify(&mut self, id: u32, host: &mut dyn Sched) -> Result<(), String> {
        let graph = &mut self.graph as *mut Graph;
        reactive::notify(graph, id, host)
    }

    pub fn on_cleanup(&mut self, id: u32) -> bool {
        reactive::on_cleanup(&mut self.graph as *mut Graph, id)
    }

    pub fn dispose_computation(&mut self, id: u32, host: &mut dyn Sched) {
        reactive::dispose(&mut self.graph as *mut Graph, id, host);
    }

    pub fn run_scheduled(&mut self, init: bool, host: &mut dyn Sched) {
        reactive::run_scheduled(&mut self.graph as *mut Graph, init, host);
    }

    pub fn graph_error(&self) -> Option<&str> {
        self.graph.error.as_deref()
    }

    pub fn take_graph_error(&mut self) -> Option<String> {
        self.graph.error.take()
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
