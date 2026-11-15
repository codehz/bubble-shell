//! Signal graph plus a double-buffered op batch.
//!
//! Scripts own a `Turn`; the host owns `Shell`. Bindings live in the host.
//! This type records which binding read which signal, marks those bindings
//! dirty on write, and coalesces the ops pushed while rerunning them. `commit`
//! only swaps buffers and encodes a batch; it never mutates the tree.

use crate::reactive::{self, Graph, Kind, Sched};
use crate::state_style::AnimSpec;

use crate::expr::ExprConst;
use crate::gesture::GestureKind;
use crate::ir::NodeDesc;
use crate::ops::{encode, Op, Value};
use crate::ui::UiCapture;

pub struct Turn {
    /// Values stay in the script host. [`Graph`] schedules memos and effects.
    graph: Graph,
    bufs: [Vec<Op>; 2],
    write: usize,
    seq: u32,
    next_handle: u32,
    next_gesture: u32,
    next_var: u32,
    next_style: u32,
    next_ui_function: u32,
}

impl Default for Turn {
    fn default() -> Self {
        Self::new()
    }
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
            next_var: 1,
            next_style: 1,
            next_ui_function: 1,
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
        for op in crate::ops::spawn_ops(desc) {
            self.push(op);
        }
        Ok(handle)
    }

    /// Spawn at a final child index; used by fragment anchors.
    pub fn spawn_at(&mut self, mut desc: NodeDesc, index: u32) -> Result<u32, String> {
        let handle = self.allocate_handle()?;
        desc.id = handle;
        for op in crate::ops::spawn_at_ops(desc, index) {
            self.push(op);
        }
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
        bump(&mut self.next_handle, "handle")
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
        let gesture = bump(&mut self.next_gesture, "gesture")?;
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

    /// Allocate a variable id and queue its creation. `source` is one of the
    /// `SOURCE_*` codes; `initial` carries the starting value for
    /// script-driven variables. The source target arrives with `bind_var`
    /// once the reference resolves.
    pub fn create_var(&mut self, source: u8, initial: Value) -> Result<u32, String> {
        let var = bump(&mut self.next_var, "var")?;
        self.push(Op::CreateVar {
            var,
            source,
            initial,
        });
        Ok(var)
    }

    /// Attach a variable's source target; the reference may resolve after the
    /// variable is created.
    pub fn bind_var(&mut self, var: u32, target: u32) {
        self.push(Op::BindVar { var, target });
    }

    pub fn dispose_var(&mut self, var: u32) {
        self.push(Op::DisposeVar { var });
    }

    /// Queue one script-driven variable write. It lands on the UI thread at
    /// the frame boundary and re-evaluates the declarations that read it.
    /// `anim` starts a tween or spring from the current displayed value;
    /// `None` lands immediately.
    pub fn set_var(&mut self, var: u32, value: Value, anim: Option<AnimSpec>) {
        self.push(Op::SetVar { var, value, anim });
    }

    /// Request a settle event for `var`. With no animation configured the
    /// variable converges as the batch applies, so the event goes out then.
    pub fn settle_var(&mut self, var: u32) {
        self.push(Op::SettleVar { var });
    }

    /// Allocate a UI program id and queue its definition.
    pub fn define_ui_function(
        &mut self,
        params: u8,
        captures: Vec<UiCapture>,
        consts: Vec<ExprConst>,
        code: Vec<u8>,
    ) -> Result<u32, String> {
        let id = bump(&mut self.next_ui_function, "ui function")?;
        self.push(Op::DefineUiFunction {
            id,
            params,
            captures,
            consts,
            code,
        });
        Ok(id)
    }

    /// Bind a UI program to one gesture event slot. Registering a program
    /// clears that slot's script handler and vice versa.
    pub fn bind_gesture_program(&mut self, gesture: u32, slot: u8, function: u32) {
        self.push(Op::BindGestureProgram {
            gesture,
            slot,
            function,
        });
    }

    /// Queue one UI call with JS-supplied arguments.
    pub fn run_ui_function(&mut self, function: u32, args: Vec<Value>) {
        self.push(Op::RunUiFunction { function, args });
    }

    /// Stop a variable's in-flight animation; the displayed value stays.
    pub fn cancel_animation(&mut self, var: u32) {
        self.push(Op::CancelVarAnim { var });
    }

    /// Allocate a style declaration id and queue its declaration under
    /// `target`. `component` 1 ViewStyle, 2 TextStyle.
    pub fn spawn_style_decl(
        &mut self,
        target: u32,
        component: u8,
        index: u32,
    ) -> Result<u32, String> {
        let decl = bump(&mut self.next_style, "style declaration")?;
        self.push(Op::SpawnStyleDecl {
            decl,
            target,
            component,
            index,
        });
        Ok(decl)
    }

    pub fn set_style_decl_prop(&mut self, decl: u32, key: impl Into<String>, value: Value) {
        self.push(Op::SetStyleDeclProp {
            decl,
            key: key.into(),
            value,
        });
    }

    pub fn move_style_decl(&mut self, decl: u32, target: u32, index: u32) {
        self.push(Op::MoveStyleDecl {
            decl,
            target,
            index,
        });
    }

    pub fn dispose_style_decl(&mut self, decl: u32) {
        self.push(Op::DisposeStyleDecl { decl });
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

/// One step of a backwards `coalesce` scan.
enum Scan {
    /// Previous write found and rewritten; drop the incoming op.
    Replace,
    /// Transparent for this op kind; keep scanning.
    Skip,
    /// Barrier for this op kind; stop scanning.
    Stop,
}

/// Shared backwards scan for the `coalesce` arms: walks the buffer from the
/// end, lets `visit` rewrite an existing op or pass over it. The per-arm skip
/// lists live in the match arms of `coalesce`, side by side, so they can be
/// compared and tightened deliberately.
fn coalesce_last(buf: &mut Vec<Op>, mut visit: impl FnMut(&mut Op) -> Scan) -> bool {
    for item in buf.iter_mut().rev() {
        match visit(item) {
            Scan::Replace => return true,
            Scan::Skip => {}
            Scan::Stop => return false,
        }
    }
    false
}

/// Allocate the current value of a counter and advance it.
fn bump(counter: &mut u32, what: &str) -> Result<u32, String> {
    let value = *counter;
    *counter = value
        .checked_add(1)
        .ok_or_else(|| format!("{what} space exhausted"))?;
    Ok(value)
}

fn coalesce(buf: &mut Vec<Op>, op: Op) {
    let handled = match &op {
        Op::SetText { handle, text } => {
            coalesce_last(buf, |item| match item {
                Op::SetText {
                    handle: existing,
                    text: slot,
                } if existing == handle => {
                    *slot = text.clone();
                    Scan::Replace
                }
                Op::SetPropColor { .. } | Op::SetProp { .. } => Scan::Skip,
                _ => Scan::Stop,
            })
        }
        Op::SetPropColor { handle, key, rgba } => {
            coalesce_last(buf, |item| match item {
                Op::SetPropColor {
                    handle: existing,
                    key: existing_key,
                    rgba: slot,
                } if existing == handle && existing_key == key => {
                    *slot = *rgba;
                    Scan::Replace
                }
                Op::SetText { .. } | Op::SetPropColor { .. } | Op::SetProp { .. } => Scan::Skip,
                _ => Scan::Stop,
            })
        }
        Op::SetProp { handle, key, value } => {
            coalesce_last(buf, |item| match item {
                Op::SetProp {
                    handle: existing,
                    key: existing_key,
                    value: slot,
                } if existing == handle && existing_key == key => {
                    *slot = value.clone();
                    Scan::Replace
                }
                Op::SetText { .. } | Op::SetPropColor { .. } | Op::SetProp { .. } => Scan::Skip,
                _ => Scan::Stop,
            })
        }
        Op::SetStyleDeclProp { decl, key, value } => {
            coalesce_last(buf, |item| match item {
                Op::SetStyleDeclProp {
                    decl: existing,
                    key: existing_key,
                    value: slot,
                } if existing == decl && existing_key == key => {
                    *slot = value.clone();
                    Scan::Replace
                }
                Op::SetText { .. } | Op::SetPropColor { .. } | Op::SetProp { .. } => Scan::Skip,
                _ => Scan::Stop,
            })
        }
        Op::SetVar { var, value, anim } => {
            coalesce_last(buf, |item| match item {
                Op::SetVar {
                    var: existing,
                    value: slot,
                    anim: slot_anim,
                } if existing == var => {
                    *slot = value.clone();
                    *slot_anim = *anim;
                    Scan::Replace
                }
                Op::SetText { .. }
                | Op::SetPropColor { .. }
                | Op::SetProp { .. }
                | Op::SetStyleDeclProp { .. }
                | Op::SetGestureProp { .. }
                | Op::SetVar { .. } => Scan::Skip,
                _ => Scan::Stop,
            })
        }
        Op::SettleVar { var } => {
            coalesce_last(buf, |item| match item {
                // A pending request already covers this variable.
                Op::SettleVar { var: existing } if existing == var => Scan::Replace,
                Op::SetText { .. }
                | Op::SetPropColor { .. }
                | Op::SetProp { .. }
                | Op::SetStyleDeclProp { .. }
                | Op::SetGestureProp { .. }
                | Op::SetVar { .. } => Scan::Skip,
                _ => Scan::Stop,
            })
        }
        Op::SetGestureProp {
            gesture,
            key,
            value,
        } => {
            coalesce_last(buf, |item| match item {
                Op::SetGestureProp {
                    gesture: existing,
                    key: existing_key,
                    value: slot,
                } if existing == gesture && existing_key == key => {
                    *slot = value.clone();
                    Scan::Replace
                }
                Op::SetText { .. } | Op::SetPropColor { .. } | Op::SetProp { .. } => Scan::Skip,
                _ => Scan::Stop,
            })
        }
        _ => false,
    };
    if !handled {
        buf.push(op);
    }
}
