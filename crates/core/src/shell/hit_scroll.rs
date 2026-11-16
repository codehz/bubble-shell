use crate::gesture::{GestureEvent, PointerAction, PointerInput, TAP_SLOP};
use crate::ir::Kind;
use crate::layout::{LayoutRequest, LayoutSnapshot};

use super::paint::{axis_bounds, Bounds};
use super::tree::Node;
use super::{ScrollOutcome, Shell, VisualPress};

/// Inverse of one node's paint transform (`PaintCmd::PushTransform`): maps
/// points and clip rects from the parent's paint frame into the layout
/// coordinates the node paints with. Paint maps `p` to
/// `center + translate + R(rotate) * scale * (p - center)`; this undoes it.
/// Rects go through their corners, so a rotated clip widens to the corner
/// AABB — that can only over-accept a point, never drop a visible one.
/// A rounded radius is kept only while that map stays axis-aligned, and
/// scales with the mapped edge. Rotation drops it so the widened box does
/// not grow circular corners the source clip never had.
struct Untransform {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl Untransform {
    /// `None` when the transform collapses the subtree (zero scale): paint
    /// draws nothing there, so hit testing must not hit either.
    fn new(
        cx: f32,
        cy: f32,
        translate_x: f32,
        translate_y: f32,
        scale: f32,
        rotate: f32,
    ) -> Option<Self> {
        if !scale.is_finite() || scale == 0.0 {
            return None;
        }
        let (sin, cos) = rotate.sin_cos();
        let k = 1.0 / scale;
        let a = k * cos;
        let b = k * sin;
        let c = -k * sin;
        let d = k * cos;
        let hx = cx + translate_x;
        let hy = cy + translate_y;
        Some(Self {
            a,
            b,
            c,
            d,
            e: cx - a * hx - b * hy,
            f: cy - c * hx - d * hy,
        })
    }

    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.b * y + self.e,
            self.c * x + self.d * y + self.f,
        )
    }

    fn bounds(&self, bounds: Bounds) -> Bounds {
        let (ax, ay) = self.point(bounds.x0, bounds.y0);
        let (bx, by) = self.point(bounds.x1, bounds.y0);
        let (cx, cy) = self.point(bounds.x0, bounds.y1);
        let (dx, dy) = self.point(bounds.x1, bounds.y1);
        let x0 = ax.min(bx).min(cx).min(dx);
        let y0 = ay.min(by).min(cy).min(dy);
        let x1 = ax.max(bx).max(cx).max(dx);
        let y1 = ay.max(by).max(cy).max(dy);
        let axis_aligned = (ay - by).abs() <= 1.0e-3 && (ax - cx).abs() <= 1.0e-3;
        let ow = (bounds.x1 - bounds.x0).abs();
        let radius = if bounds.radius > 0.0 && axis_aligned && ow > f32::EPSILON {
            bounds.radius * ((ax - bx).abs() / ow)
        } else {
            0.0
        };
        Bounds {
            x0,
            y0,
            x1,
            y1,
            radius,
        }
    }
}

/// `true` when `(x, y)` is inside the rounded rectangle at `(rx, ry, rw, rh)`.
/// `radius` is clamped to half the shorter side. A point in the central cross
/// is inside; a point in a corner is inside when it falls within that corner's
/// circle. Far edges are exclusive, matching a rectangular [`Bounds::contains`].
pub(super) fn contains_rounded(
    x: f32,
    y: f32,
    rx: f32,
    ry: f32,
    rw: f32,
    rh: f32,
    radius: f32,
) -> bool {
    if !(x >= rx && y >= ry && x < rx + rw && y < ry + rh) {
        return false;
    }
    let r = radius.max(0.0).min(rw.min(rh) * 0.5);
    if r <= 0.0 {
        return true;
    }
    if (x >= rx + r && x < rx + rw - r) || (y >= ry + r && y < ry + rh - r) {
        return true;
    }
    let cx = if x < rx + r { rx + r } else { rx + rw - r };
    let cy = if y < ry + r { ry + r } else { ry + rh - r };
    let dx = x - cx;
    let dy = y - cy;
    dx * dx + dy * dy <= r * r
}

impl Shell {
    /// Handler on the hit node, or the nearest ancestor that has one.
    /// A node without a handler still occludes nodes behind it.
    pub fn tap(&self, x: f32, y: f32) -> Option<u32> {
        let mut path = Vec::new();
        if !self.hit_path(1, x, y, &mut path) {
            return None;
        }
        path.iter().find_map(|id| {
            let handler = self.node(*id).ok()?.handler;
            if handler == 0 {
                None
            } else {
                Some(handler)
            }
        })
    }

    /// Feed one normalized pointer sample. `Down` fixes the real hit path;
    /// move/up/cancel reuse it. Scroll deltas produced by the recognizer are
    /// applied to the frozen chain immediately, before the host reads paint.
    ///
    /// Field hits are not recognized here. Call [`Self::pointer_with_layout`]
    /// when the host can answer text layout.
    pub fn pointer(&mut self, input: PointerInput) {
        let host: Option<&mut dyn FnMut(&LayoutRequest<'_>) -> Result<LayoutSnapshot, String>> =
            None;
        self.pointer_with_layout(input, host);
    }

    /// [`Self::pointer`] plus the layout host field gestures need to place a
    /// caret or hit-test a text offset. `None` leaves field samples on the
    /// recognizer path.
    pub fn pointer_with_layout(
        &mut self,
        mut input: PointerInput,
        host: Option<&mut dyn FnMut(&LayoutRequest<'_>) -> Result<LayoutSnapshot, String>>,
    ) {
        let previous = self.now_ms;
        self.now_ms = input.time_ms;
        if self.scrollbar_visibility_changed(previous, self.now_ms) {
            self.paint_dirty.set(true);
        }
        if input.action == PointerAction::Down {
            input.path.clear();
            self.hit_path(1, input.x, input.y, &mut input.path);
            self.last_pointer_kind = input.pointer_kind;
        }
        if self.claim_scrollbar(&input) {
            return;
        }
        let field_claimed = if self.field_owns(input.id) && input.action != PointerAction::Down {
            match host {
                Some(host) => self.handle_field_pointer(&input, host),
                None => {
                    let mut missing = |_: &LayoutRequest<'_>| Err(String::from("no layout host"));
                    self.handle_field_pointer(&input, &mut missing);
                }
            }
            true
        } else if input.action == PointerAction::Down {
            match host {
                Some(host) => self.try_field_pointer(&input, host),
                None => false,
            }
        } else {
            false
        };
        if field_claimed {
            let changed = self.track_press(&input, false);
            self.flush_gesture_scroll();
            self.sample_sources();
            self.drain_ui_work();
            if changed {
                self.paint_dirty.set(true);
            }
            return;
        }
        let mut changed = self.track_press(&input, true);
        self.recognizer.pointer(input);
        if self.visual.gesture
            && self.visual.target != 0
            && self.recognizer.visual_press_cancelled()
        {
            changed |= self.clear_press();
        }
        self.flush_gesture_scroll();
        self.sample_sources();
        self.drain_ui_work();
        if changed {
            self.paint_dirty.set(true);
        }
    }

    /// Pressed feedback only: no recognition, no tap, no scroll. Hosts use it
    /// when their own pointer path owns the sample, so the same physical touch
    /// is never fed twice.
    pub fn visual_pointer(&mut self, mut input: PointerInput) {
        if input.action == PointerAction::Down {
            input.path.clear();
            self.hit_path(1, input.x, input.y, &mut input.path);
            self.last_pointer_kind = input.pointer_kind;
        }
        if self.track_press(&input, false) {
            self.sample_sources();
            self.paint_dirty.set(true);
        }
    }

    pub fn advance_gestures(&mut self, now_ms: f64) {
        let previous = self.now_ms;
        self.recognizer.advance(now_ms);
        self.advance_field_pointer(now_ms);
        self.advance_animations(now_ms);
        if self.scrollbar_visibility_changed(previous, self.now_ms) {
            self.paint_dirty.set(true);
        }
        let mut changed = false;
        if self.visual.gesture
            && self.visual.target != 0
            && self.recognizer.visual_press_cancelled()
        {
            changed = self.clear_press();
        }
        self.flush_gesture_scroll();
        if changed {
            self.sample_sources();
            self.paint_dirty.set(true);
        }
        self.drain_ui_work();
    }

    /// Drain recognition events in order. The host dispatches them to the
    /// script thread; nothing here calls JS.
    /// Host pause/focus loss: drop touches, timers, reserves, and ownership.
    pub fn cancel_gestures(&mut self, reason: u8) -> Vec<GestureEvent> {
        self.scrollbar_drag = None;
        self.cancel_field_pointer();
        self.recognizer.cancel(reason);
        self.flush_gesture_scroll();
        if self.reset_press() {
            self.sample_sources();
            self.paint_dirty.set(true);
        }
        self.take_gesture_events()
    }

    /// Nearest node in the hit path that owns pressed feedback for a fresh
    /// touch. Interest wins regardless of click targets deeper in the path:
    /// tapping a nested button still drives an ancestor's pressed style.
    fn press_target(&self, path: &[u32]) -> u32 {
        let modal = self.active_modal();
        for id in path {
            if !self.interaction_available(*id, modal) {
                continue;
            }
            if self.state_styles.pressed_interest(*id) {
                return *id;
            }
        }
        0
    }

    fn clear_press(&mut self) -> bool {
        let had = self.visual.target != 0;
        self.visual.target = 0;
        had
    }

    fn reset_press(&mut self) -> bool {
        let had = self.visual.target != 0
            || self.visual.owner != 0
            || !self.visual.pointers.is_empty()
            || self.visual.gesture;
        self.visual = VisualPress::default();
        had
    }

    /// Track one eligible pointer sample. Returns whether pressed feedback
    /// changed. `gesture` marks a sample the recognizer also saw.
    fn track_press(&mut self, input: &PointerInput, gesture: bool) -> bool {
        // Eligibility gates starting a sequence: a mouse sample only counts
        // while the primary button is held, and a released mouse pointer
        // reports `buttons == 0`. Up/Move/Cancel of a tracked pointer must
        // always pass so the sequence can end.
        if input.action == PointerAction::Down && !input.eligible() {
            return false;
        }
        match input.action {
            PointerAction::Down => {
                if self.visual.pointers.contains(&input.id) {
                    return false;
                }
                let first = self.visual.pointers.is_empty();
                self.visual.pointers.push(input.id);
                if !first {
                    return self.clear_press();
                }
                self.visual.owner = input.id;
                self.visual.start_x = input.x;
                self.visual.start_y = input.y;
                self.visual.gesture = gesture;
                let target = self.press_target(&input.path);
                if target == 0 {
                    return false;
                }
                self.visual.target = target;
                true
            }
            PointerAction::Move => {
                if input.id != self.visual.owner || self.visual.target == 0 {
                    return false;
                }
                let distance = (input.x - self.visual.start_x).hypot(input.y - self.visual.start_y);
                if distance > TAP_SLOP {
                    return self.clear_press();
                }
                false
            }
            PointerAction::Up => {
                if !self.visual.pointers.contains(&input.id) {
                    return false;
                }
                self.visual.pointers.retain(|held| *held != input.id);
                let changed = input.id == self.visual.owner && self.clear_press();
                if self.visual.pointers.is_empty() {
                    self.visual.owner = 0;
                    self.visual.gesture = false;
                }
                changed
            }
            PointerAction::Cancel => self.reset_press(),
        }
    }

    /// Re-check the pressed target after a configuration or visibility change.
    /// A node that lost its click family, its pressed style, or its
    /// availability drops the state instead of handing the touch elsewhere.
    /// Regaining feedback does not revive a dropped touch.
    pub(super) fn sync_press_target(&mut self) {
        let target = self.visual.target;
        if target == 0 {
            return;
        }
        let alive = self.interaction_available(target, self.active_modal())
            && (self.state_styles.pressed_feedback(target)
                || self
                    .recognizer
                    .click_target(std::slice::from_ref(&target))
                    .is_some());
        if !alive {
            self.visual.target = 0;
            self.sample_sources();
            self.paint_dirty.set(true);
        }
    }

    pub fn set_gesture_trace(&mut self, enabled: bool) {
        self.recognizer.set_trace(enabled);
    }

    pub fn take_gesture_trace(&mut self) -> Vec<String> {
        self.recognizer.take_trace()
    }

    pub fn take_gesture_events(&mut self) -> Vec<GestureEvent> {
        self.recognizer.drain_events()
    }

    fn flush_gesture_scroll(&mut self) {
        let requests = self.recognizer.drain_scroll();
        for request in requests {
            self.scroll_path(&request.path, request.dx, request.dy);
        }
    }

    pub(super) fn hit_path(&self, id: u32, x: f32, y: f32, out: &mut Vec<u32>) -> bool {
        self.hit_clipped(id, x, y, self.viewport_bounds(), out)
    }

    fn viewport_bounds(&self) -> Bounds {
        Bounds {
            x0: 0.0,
            y0: 0.0,
            x1: self.viewport.width,
            y1: self.viewport.height,
            radius: 0.0,
        }
    }

    /// Nodes under the point from deepest to root. `(x, y)` and `clip`
    /// arrive in the parent's paint frame; this node's own transform
    /// (translate, scale, rotate around the node center, matching
    /// `PaintCmd::PushTransform`) is undone first, so every bounds test here
    /// uses the layout coordinates the node paints with. Children receive
    /// the already-mapped point and clip. Hidden and `display: none`
    /// subtrees miss. A clipped axis stops at the padding box (the outer
    /// box); a visible axis stays open inside the viewport and ancestor
    /// clips.
    fn hit_clipped(&self, id: u32, x: f32, y: f32, clip: Bounds, out: &mut Vec<u32>) -> bool {
        let Some(node) = self.nodes.get(id as usize).and_then(Option::as_ref) else {
            return false;
        };
        if !node.painted() {
            return false;
        }
        let paint = self.effective_paint_style(id, node);
        let (x, y, clip) = if paint.translate_x != 0.0
            || paint.translate_y != 0.0
            || paint.scale != 1.0
            || paint.rotate != 0.0
        {
            match Untransform::new(
                node.x + node.w / 2.0,
                node.y + node.h / 2.0,
                paint.translate_x,
                paint.translate_y,
                paint.scale,
                paint.rotate,
            ) {
                Some(map) => {
                    let (x, y) = map.point(x, y);
                    (x, y, map.bounds(clip))
                }
                // Zero scale collapses the subtree to nothing.
                None => return false,
            }
        } else {
            (x, y, clip)
        };
        if !clip.contains(x, y) {
            return false;
        }
        // A fully visible node must not stamp its corner radius onto the
        // viewport. Radius applies only once an axis actually clips.
        let clip_radius = if node.overflow_x == taffy::style::Overflow::Visible
            && node.overflow_y == taffy::style::Overflow::Visible
        {
            0.0
        } else {
            paint.radius
        };
        let next = clip.intersect(axis_bounds(
            node.overflow_x,
            node.overflow_y,
            node.x,
            node.y,
            node.w,
            node.h,
            self.viewport.width,
            self.viewport.height,
            clip_radius,
        ));
        let (nx, ny, nw, nh) = (node.x, node.y, node.w, node.h);
        let count = node.children.len();
        let scrolls_x = node.scrolls_x();
        let scrolls_y = node.scrolls_y();
        let scroll_left = node.scroll_left;
        let scroll_top = node.scroll_top;
        if (scrolls_x || scrolls_y)
            && next.contains(x, y)
            && self.hit_scrollbar(
                id,
                x,
                y,
                nx,
                ny,
                nw,
                nh,
                scroll_left,
                scroll_top,
                scrolls_x,
                scrolls_y,
                out,
            )
        {
            return true;
        }
        if next.contains(x, y) {
            for index in (0..count).rev() {
                let child = self.nodes[id as usize]
                    .as_ref()
                    .expect("hit child")
                    .children[index];
                if self.hit_clipped(child, x, y, next, out) {
                    out.push(id);
                    return true;
                }
            }
        }
        if x >= nx && y >= ny && x < nx + nw && y < ny + nh && next.contains(x, y) {
            out.push(id);
            true
        } else {
            false
        }
    }

    /// Wheel/trackpad input: route each axis through the hit path from the
    /// deepest scroll container outward. Returns whether an offset changed.
    pub fn scroll_wheel(&mut self, x: f32, y: f32, dx: f32, dy: f32) -> bool {
        self.scroll_chain(x, y, dx, dy).changed()
    }

    /// Route a normalized delta through the real hit path. x and y are
    /// independent; each container consumes only its own enabled axis and a
    /// `contain` boundary stops that axis after its own consumption.
    pub fn scroll_chain(&mut self, x: f32, y: f32, dx: f32, dy: f32) -> ScrollOutcome {
        let mut path = Vec::new();
        if !self.hit_path(1, x, y, &mut path) {
            return ScrollOutcome {
                remaining_x: dx,
                remaining_y: dy,
                ..ScrollOutcome::default()
            };
        }
        self.scroll_path(&path, dx, dy)
    }

    /// Apply an already-frozen path. Used by touch scrolling after the down
    /// sample fixed the target chain.
    pub fn scroll_path(&mut self, path: &[u32], dx: f32, dy: f32) -> ScrollOutcome {
        let mut outcome = ScrollOutcome {
            remaining_x: dx,
            remaining_y: dy,
            ..ScrollOutcome::default()
        };
        for id in path {
            let Ok(node) = self.node(*id) else {
                continue;
            };
            let scroll_x = node.scrolls_x();
            let scroll_y = node.scrolls_y();
            let chain_x = node.chain_x;
            let chain_y = node.chain_y;
            let limit_x = if scroll_x {
                self.max_scroll_x(node)
            } else {
                0.0
            };
            let limit_y = if scroll_y {
                self.max_scroll_y(node)
            } else {
                0.0
            };
            let mut moved = false;
            if scroll_x && outcome.remaining_x != 0.0 {
                let node = self.node_mut(*id).expect("scroll node");
                let before = node.scroll_left;
                let next = (before + outcome.remaining_x).clamp(0.0, limit_x);
                node.scroll_left = next;
                let consumed = next - before;
                outcome.consumed_x += consumed;
                outcome.remaining_x -= consumed;
                if consumed != 0.0 {
                    moved = true;
                }
            }
            if scroll_y && outcome.remaining_y != 0.0 {
                let node = self.node_mut(*id).expect("scroll node");
                let before = node.scroll_top;
                let next = (before + outcome.remaining_y).clamp(0.0, limit_y);
                node.scroll_top = next;
                let consumed = next - before;
                outcome.consumed_y += consumed;
                outcome.remaining_y -= consumed;
                if consumed != 0.0 {
                    moved = true;
                }
            }
            if moved {
                self.last_scroll_time.insert(*id, self.now_ms);
            }
            if chain_x && scroll_x {
                outcome.truncated_x = true;
                outcome.remaining_x = 0.0;
            }
            if chain_y && scroll_y {
                outcome.truncated_y = true;
                outcome.remaining_y = 0.0;
            }
        }
        if outcome.changed() {
            self.sample_sources();
            self.paint_dirty.set(true);
        }
        if self.recognizer.is_tracing() {
            let trace = outcome;
            self.recognizer.trace(|| {
                format!(
                    "scroll consumed x={} y={} remaining x={} y={} truncated x={} y={}",
                    trace.consumed_x,
                    trace.consumed_y,
                    trace.remaining_x,
                    trace.remaining_y,
                    trace.truncated_x,
                    trace.truncated_y
                )
            });
        }
        outcome
    }

    pub(super) fn scroll_to(&mut self, id: u32, x: f32, y: f32) -> Result<(), String> {
        let (scroll_x, scroll_y) = {
            let node = self.node(id)?;
            (node.scrolls_x(), node.scrolls_y())
        };
        if !scroll_x && !scroll_y {
            return Err(format!("handle {id} is not a scroll container"));
        }
        let limit_x = if scroll_x {
            self.max_scroll_x(self.node(id)?)
        } else {
            0.0
        };
        let limit_y = if scroll_y {
            self.max_scroll_y(self.node(id)?)
        } else {
            0.0
        };
        let changed = {
            let node = self.node_mut(id)?;
            let next_x = x.clamp(0.0, limit_x);
            let next_y = y.clamp(0.0, limit_y);
            let changed = node.scroll_left != next_x || node.scroll_top != next_y;
            node.scroll_left = next_x;
            node.scroll_top = next_y;
            changed
        };
        if changed {
            self.last_scroll_time.insert(id, self.now_ms);
        }
        self.sample_sources();
        self.paint_dirty.set(true);
        Ok(())
    }

    pub(super) fn max_scroll_x(&self, node: &Node) -> f32 {
        if node.kind == Kind::Field {
            let (visible, _) = self.field_viewport(node);
            return (node.content_w - visible).max(0.0);
        }
        let Ok(layout) = self.taffy.layout(node.taffy) else {
            return 0.0;
        };
        // Rect origin is the padding edge and already includes end padding.
        layout.scroll_width()
    }

    pub(super) fn max_scroll_y(&self, node: &Node) -> f32 {
        if node.kind == Kind::Field {
            let (_, visible) = self.field_viewport(node);
            return (node.content_h - visible).max(0.0);
        }
        let Ok(layout) = self.taffy.layout(node.taffy) else {
            return 0.0;
        };
        layout.scroll_height()
    }

    fn field_viewport(&self, node: &Node) -> (f32, f32) {
        let Ok(layout) = self.taffy.layout(node.taffy) else {
            return (0.0, 0.0);
        };
        (
            (layout.size.width - layout.padding.left - layout.padding.right).max(0.0),
            (layout.size.height - layout.padding.top - layout.padding.bottom).max(0.0),
        )
    }

    pub(super) fn scrollbar_visible(&self, auto_hide: bool, container: u32) -> bool {
        if !auto_hide {
            return true;
        }
        let last = self
            .last_scroll_time
            .get(&container)
            .copied()
            .unwrap_or(0.0);
        self.now_ms - last < 1000.0
    }

    fn scrollbar_visibility_changed(&self, previous: f64, now: f64) -> bool {
        if previous == now {
            return false;
        }
        for index in 0..self.scrollbars.decl_slots() {
            let (auto_hide, target) = {
                let Some(bar) = self.scrollbars.at(index) else {
                    continue;
                };
                (bar.auto_hide, bar.target)
            };
            if !auto_hide {
                continue;
            }
            let last = self.last_scroll_time.get(&target).copied().unwrap_or(0.0);
            if (previous - last < 1000.0) != (now - last < 1000.0) {
                return true;
            }
        }
        false
    }

    fn claim_scrollbar(&mut self, input: &PointerInput) -> bool {
        if let Some(drag) = self.scrollbar_drag {
            if drag.pointer != input.id {
                return false;
            }
            match input.action {
                PointerAction::Move => {
                    self.apply_scrollbar_drag(drag, input.x, input.y);
                    true
                }
                PointerAction::Up | PointerAction::Cancel => {
                    self.scrollbar_drag = None;
                    true
                }
                PointerAction::Down => true,
            }
        } else if input.action == PointerAction::Down {
            self.begin_scrollbar_drag(input)
        } else {
            false
        }
    }

    fn begin_scrollbar_drag(&mut self, input: &PointerInput) -> bool {
        let Some(&id) = input.path.first() else {
            return false;
        };
        // `id` is a declaration when the thumb was hit. A node handle that
        // happens to share the number is rejected unless the point is on that
        // declaration's thumb.
        let Some((container, axis, margin, thickness, min_length, auto_hide, interactive)) =
            self.scrollbars.scrollbar(id).map(|bar| {
                (
                    bar.target,
                    bar.axis,
                    bar.margin,
                    bar.width,
                    bar.min_length,
                    bar.auto_hide,
                    bar.interactive,
                )
            })
        else {
            return false;
        };
        if !interactive {
            return false;
        }
        let vertical = axis == 0;
        let (extent, start_scroll, scrolls, origin_x, origin_y, width, height) = {
            let Ok(parent) = self.node(container) else {
                return false;
            };
            (
                if vertical { parent.h } else { parent.w },
                if vertical {
                    parent.scroll_top
                } else {
                    parent.scroll_left
                },
                if vertical {
                    parent.scrolls_y()
                } else {
                    parent.scrolls_x()
                },
                parent.x,
                parent.y,
                parent.w,
                parent.h,
            )
        };
        if !scrolls || !self.scrollbar_visible(auto_hide, container) {
            return false;
        }
        let max_scroll = match self.node(container) {
            Ok(node) if vertical => self.max_scroll_y(node),
            Ok(node) => self.max_scroll_x(node),
            Err(_) => return false,
        };
        let track = extent - 2.0 * margin;
        if track <= 0.0 || max_scroll <= 0.0 {
            return false;
        }
        let Some(thumb) = super::scrollbar_thumb(
            vertical,
            origin_x,
            origin_y,
            width,
            height,
            start_scroll,
            max_scroll,
            thickness,
            margin,
            min_length,
        ) else {
            return false;
        };
        if input.x < thumb.x
            || input.x >= thumb.x + thumb.w
            || input.y < thumb.y
            || input.y >= thumb.y + thumb.h
        {
            return false;
        }
        self.scrollbar_drag = Some(super::ScrollbarDrag {
            pointer: input.id,
            scrollbar: id,
            container,
            axis,
            start_pointer: if vertical { input.y } else { input.x },
            start_scroll,
            track_length: track,
            max_scroll,
        });
        self.last_scroll_time.insert(container, self.now_ms);
        self.paint_dirty.set(true);
        true
    }

    fn apply_scrollbar_drag(&mut self, drag: super::ScrollbarDrag, x: f32, y: f32) {
        let Ok(container) = self.node(drag.container) else {
            self.scrollbar_drag = None;
            return;
        };
        let extent = if drag.axis == 0 {
            container.h
        } else {
            container.w
        };
        let scroll_left = container.scroll_left;
        let scroll_top = container.scroll_top;
        let min_length = self
            .scrollbars
            .scrollbar(drag.scrollbar)
            .map(|bar| bar.min_length)
            .unwrap_or(0.0);
        let content = drag.max_scroll + extent;
        let ratio = if content > 0.0 {
            (extent / content).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let thumb = (drag.track_length * ratio)
            .max(min_length)
            .min(drag.track_length.max(0.0));
        let scrollable = drag.track_length - thumb;
        let pointer = if drag.axis == 0 { y } else { x };
        let delta = pointer - drag.start_pointer;
        let scroll_delta = if scrollable > 0.0 {
            delta * drag.max_scroll / scrollable
        } else {
            0.0
        };
        let new_scroll = (drag.start_scroll + scroll_delta).clamp(0.0, drag.max_scroll);
        self.last_scroll_time.insert(drag.container, self.now_ms);
        if drag.axis == 0 {
            let _ = self.scroll_to(drag.container, scroll_left, new_scroll);
        } else {
            let _ = self.scroll_to(drag.container, new_scroll, scroll_top);
        }
    }

    fn hit_scrollbar(
        &self,
        id: u32,
        x: f32,
        y: f32,
        origin_x: f32,
        origin_y: f32,
        width: f32,
        height: f32,
        scroll_left: f32,
        scroll_top: f32,
        scrolls_x: bool,
        scrolls_y: bool,
        out: &mut Vec<u32>,
    ) -> bool {
        let count = self.scrollbars.ids(id).map(|ids| ids.len()).unwrap_or(0);
        if count == 0 {
            return false;
        }
        let max_x = if scrolls_x {
            self.max_scroll_x(self.node(id).expect("scrollbar parent"))
        } else {
            0.0
        };
        let max_y = if scrolls_y {
            self.max_scroll_y(self.node(id).expect("scrollbar parent"))
        } else {
            0.0
        };
        for index in (0..count).rev() {
            let bar_id = self.scrollbars.ids(id).expect("scrollbar ids")[index];
            let Some((vertical, auto_hide, thickness, margin, min_length)) =
                self.scrollbars.scrollbar(bar_id).and_then(|bar| {
                    if !bar.interactive {
                        None
                    } else {
                        Some((
                            bar.axis == 0,
                            bar.auto_hide,
                            bar.width,
                            bar.margin,
                            bar.min_length,
                        ))
                    }
                })
            else {
                continue;
            };
            let scrollable = if vertical {
                scrolls_y && max_y > 0.0
            } else {
                scrolls_x && max_x > 0.0
            };
            if !scrollable || !self.scrollbar_visible(auto_hide, id) {
                continue;
            }
            let Some(thumb) = super::scrollbar_thumb(
                vertical,
                origin_x,
                origin_y,
                width,
                height,
                if vertical { scroll_top } else { scroll_left },
                if vertical { max_y } else { max_x },
                thickness,
                margin,
                min_length,
            ) else {
                continue;
            };
            if x >= thumb.x && x < thumb.x + thumb.w && y >= thumb.y && y < thumb.y + thumb.h {
                out.push(bar_id);
                out.push(id);
                return true;
            }
        }
        false
    }
}
