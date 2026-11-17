//! Per-node style tweens. Config lives here, not on the node or its
//! declarations. Paint reads the displayed value; the merged target stays
//! in [`super::vars`] as the resolved style.

use std::collections::{HashMap, HashSet};

use crate::expr::ExprValue;
use crate::ir::Kind;
use crate::ops::Op;
use crate::state_style::{
    transition_slot_allowed, transition_slot_key, AnimSpec, AnimState, TransitionSpec, SLOT_BACKGROUND,
    SLOT_COLOR, SLOT_COUNT, SLOT_OPACITY, SLOT_RADIUS, SLOT_ROTATE, SLOT_SCALE, SLOT_TRANSLATE_X,
    SLOT_TRANSLATE_Y,
};

use super::paint::EffectivePaintStyle;
use super::Shell;

#[derive(Clone, Debug)]
struct StyleAnim {
    state: AnimState,
    /// Batch-created tweens wait for the first `advance_animations` so a
    /// stale `Shell::now_ms` cannot finish them before the host ticks.
    awaiting_first_tick: bool,
}

#[derive(Clone, Debug)]
struct TransitionSlot {
    spec: TransitionSpec,
    target: ExprValue,
    displayed: ExprValue,
    anim: Option<StyleAnim>,
}

#[derive(Clone, Debug)]
struct TransitionState {
    slots: [Option<TransitionSlot>; SLOT_COUNT],
}

impl Default for TransitionState {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
        }
    }
}

#[derive(Default)]
pub(super) struct TransitionRuntime {
    nodes: HashMap<u32, TransitionState>,
    dirty: HashSet<u32>,
    active: HashSet<u32>,
    /// Reused id buffer for ticks and reconcile. Not a second source of truth.
    scratch: Vec<u32>,
    /// Set for the whole `apply_ops` call. Inner sampling must not reconcile.
    batch: bool,
    spawned: HashSet<u32>,
    batch_nodes: HashSet<u32>,
    baseline: HashMap<u32, EffectivePaintStyle>,
    /// `Some(None)` is an explicit disable; missing means the batch did not write it.
    staged: HashMap<(u32, u8), Option<TransitionSpec>>,
}

fn paint_slot(style: &EffectivePaintStyle, slot: usize) -> ExprValue {
    match slot {
        SLOT_BACKGROUND => ExprValue::Color(style.background),
        SLOT_COLOR => ExprValue::Color(style.color),
        SLOT_OPACITY => ExprValue::Num(style.opacity),
        SLOT_RADIUS => ExprValue::Num(style.radius),
        SLOT_TRANSLATE_X => ExprValue::Num(style.translate_x),
        SLOT_TRANSLATE_Y => ExprValue::Num(style.translate_y),
        SLOT_SCALE => ExprValue::Num(style.scale),
        SLOT_ROTATE => ExprValue::Num(style.rotate),
        _ => ExprValue::Num(0.0),
    }
}

fn capture_resolved(shell: &Shell, id: u32) -> Option<EffectivePaintStyle> {
    let node = shell.nodes.get(id as usize)?.as_ref()?;
    Some(shell.resolved_paint_style(id, node))
}

fn apply_display(style: &mut EffectivePaintStyle, slot: usize, value: ExprValue) {
    match (slot, value) {
        (SLOT_BACKGROUND, ExprValue::Color(color)) => style.background = color,
        (SLOT_COLOR, ExprValue::Color(color)) => style.color = color,
        (SLOT_OPACITY, ExprValue::Num(value)) => style.opacity = value,
        (SLOT_RADIUS, ExprValue::Num(value)) => style.radius = value,
        (SLOT_TRANSLATE_X, ExprValue::Num(value)) => style.translate_x = value,
        (SLOT_TRANSLATE_Y, ExprValue::Num(value)) => style.translate_y = value,
        (SLOT_SCALE, ExprValue::Num(value)) => style.scale = value,
        (SLOT_ROTATE, ExprValue::Num(value)) => style.rotate = value,
        _ => {}
    }
}

fn start_tween(
    spec: TransitionSpec,
    from: ExprValue,
    to: ExprValue,
    now_ms: f64,
    awaiting: bool,
) -> StyleAnim {
    StyleAnim {
        state: AnimState::new(
            AnimSpec::Tween {
                duration_ms: spec.duration_ms,
                easing: spec.easing,
            },
            from,
            to,
            now_ms,
        ),
        awaiting_first_tick: awaiting,
    }
}

/// Sample a non-tick redirect to `now_ms`. Awaiting tweens stay on `from`.
fn sample_slot(slot: &mut TransitionSlot, now_ms: f64) {
    let Some(anim) = slot.anim.as_mut() else {
        return;
    };
    if anim.awaiting_first_tick {
        slot.displayed = anim.state.from;
        return;
    }
    let (next, converged) = anim.state.step(now_ms);
    if let Some(next) = next {
        slot.displayed = next;
    }
    if converged {
        slot.displayed = anim.state.to;
        slot.anim = None;
    }
}

/// One tick. Returns `(still running, displayed changed)`.
fn step_slot(slot: &mut TransitionSlot, now_ms: f64) -> (bool, bool) {
    let (next, converged, to) = {
        let Some(anim) = slot.anim.as_mut() else {
            return (false, false);
        };
        if anim.awaiting_first_tick {
            anim.state.start_ms = now_ms;
            anim.state.last_ms = now_ms;
            anim.awaiting_first_tick = false;
        }
        let (next, converged) = anim.state.step(now_ms);
        (next, converged, anim.state.to)
    };
    let mut changed = false;
    if let Some(next) = next {
        if slot.displayed != next {
            slot.displayed = next;
            changed = true;
        }
    }
    if converged {
        slot.anim = None;
        if slot.displayed != to {
            slot.displayed = to;
            changed = true;
        }
    }
    (slot.anim.is_some(), changed)
}


impl Shell {
    pub(crate) fn style_transition_pending(&self) -> bool {
        !self.transitions.active.is_empty()
    }
    pub(super) fn commit_style_transitions(&mut self) {
        if !self.transitions.batch {
            self.reconcile_style_transitions();
        }
    }


    pub(super) fn overlay_style_transition(&self, id: u32, style: &mut EffectivePaintStyle) {
        let Some(state) = self.transitions.nodes.get(&id) else {
            return;
        };
        for (slot, slot_state) in state.slots.iter().enumerate() {
            let Some(slot_state) = slot_state else {
                continue;
            };
            apply_display(style, slot, slot_state.displayed);
        }
    }

    pub(super) fn mark_transition_target_dirty(&mut self, target: u32) {
        if self.transitions.nodes.contains_key(&target) || self.transitions.batch_nodes.contains(&target)
        {
            self.transitions.dirty.insert(target);
        }
    }

    pub(super) fn drop_style_transition(&mut self, id: u32) {
        self.transitions.nodes.remove(&id);
        self.transitions.dirty.remove(&id);
        self.transitions.active.remove(&id);
        self.transitions.baseline.remove(&id);
        self.transitions.spawned.remove(&id);
        self.transitions.batch_nodes.remove(&id);
        self.transitions
            .staged
            .retain(|(handle, _), _| *handle != id);
    }

    pub(super) fn begin_style_transition_batch(&mut self, ops: &[Op]) {
        self.transitions.batch = true;
        for op in ops {
            if let Op::Spawn { handle, .. } | Op::SpawnAt { handle, .. } = op {
                self.transitions.spawned.insert(*handle);
            }
        }
        let mut pending_baseline = Vec::new();
        for op in ops {
            if let Op::SetStyleTransition { handle, .. } = op {
                self.transitions.batch_nodes.insert(*handle);
                if self.transitions.spawned.contains(handle)
                    || self.transitions.baseline.contains_key(handle)
                {
                    continue;
                }
                pending_baseline.push(*handle);
            }
        }
        for handle in pending_baseline {
            if let Some(style) = capture_resolved(self, handle) {
                self.transitions.baseline.insert(handle, style);
            }
        }
    }

    pub(super) fn stage_style_transition(
        &mut self,
        handle: u32,
        slot: u8,
        spec: Option<TransitionSpec>,
    ) -> Result<(), String> {
        let kind = self.node(handle)?.kind;
        if !transition_slot_allowed(kind, slot) {
            return Err(match transition_slot_key(slot) {
                Some(name) => format!("unsupported transition property {name}"),
                None => format!("unsupported transition slot {slot}"),
            });
        }
        self.transitions.batch_nodes.insert(handle);
        self.transitions.staged.insert((handle, slot), spec);
        Ok(())
    }

    pub(super) fn finish_style_transition_batch(&mut self, commit: bool) {
        if !commit {
            self.transitions.staged.clear();
            self.transitions.baseline.clear();
            self.transitions.spawned.clear();
            self.transitions.batch_nodes.clear();
        }
        self.reconcile_style_transitions();
        self.transitions.batch = false;
        self.transitions.staged.clear();
        self.transitions.baseline.clear();
        self.transitions.spawned.clear();
        self.transitions.batch_nodes.clear();
    }

    pub(super) fn step_style_transitions(&mut self, now_ms: f64) {
        let mut ids = std::mem::take(&mut self.transitions.scratch);
        ids.clear();
        ids.extend(self.transitions.active.iter().copied());
        for index in 0..ids.len() {
            let id = ids[index];
            if self.step_style_node(id, now_ms) {
                self.paint_dirty.set(true);
            }
        }
        self.transitions.scratch = ids;
    }

    fn step_style_node(&mut self, id: u32, now_ms: f64) -> bool {
        let Some(state) = self.transitions.nodes.get_mut(&id) else {
            self.transitions.active.remove(&id);
            return false;
        };
        let mut changed = false;
        let mut active = false;
        for slot in state.slots.iter_mut().flatten() {
            let (still, paint) = step_slot(slot, now_ms);
            changed |= paint;
            active |= still;
        }
        if !active {
            self.transitions.active.remove(&id);
        }
        changed
    }

    pub(super) fn reconcile_style_transitions(&mut self) {
        let mut ids = std::mem::take(&mut self.transitions.scratch);
        ids.clear();
        ids.extend(self.transitions.dirty.iter().copied());
        if self.transitions.batch {
            for &(handle, _) in self.transitions.staged.keys() {
                if !ids.contains(&handle) {
                    ids.push(handle);
                }
            }
        }
        self.transitions.dirty.clear();
        for index in 0..ids.len() {
            let id = ids[index];
            self.reconcile_style_node(id);
        }
        self.transitions.scratch = ids;
    }

    fn take_staged(&mut self, id: u32) -> [Option<Option<TransitionSpec>>; SLOT_COUNT] {
        let mut out = [None; SLOT_COUNT];
        self.transitions.staged.retain(|(handle, slot), spec| {
            if *handle != id {
                return true;
            }
            let index = *slot as usize;
            if index < SLOT_COUNT {
                out[index] = Some(*spec);
            }
            false
        });
        out
    }

    fn reconcile_style_node(&mut self, id: u32) {
        let Some(resolved) = capture_resolved(self, id) else {
            self.drop_style_transition(id);
            return;
        };
        let from_batch = self.transitions.batch;
        let spawned = self.transitions.spawned.contains(&id);
        let baseline = self.transitions.baseline.get(&id).copied();
        let staged = self.take_staged(id);
        let mut state = self.transitions.nodes.remove(&id).unwrap_or_default();
        let any_staged = staged.iter().any(Option::is_some);
        let any_state = state.slots.iter().any(Option::is_some);
        if !any_staged && !any_state {
            self.transitions.active.remove(&id);
            return;
        }
        if spawned {
            for slot in 0..SLOT_COUNT {
                let target = paint_slot(&resolved, slot);
                state.slots[slot] = staged[slot].flatten().map(|spec| TransitionSlot {
                    spec,
                    target,
                    displayed: target,
                    anim: None,
                });
            }
            self.store_transition(id, state);
            self.paint_dirty.set(true);
            return;
        }
        for slot in state.slots.iter_mut().flatten() {
            sample_slot(slot, self.now_ms);
        }
        let kind = self
            .nodes
            .get(id as usize)
            .and_then(Option::as_ref)
            .map(|node| node.kind)
            .unwrap_or(Kind::View);
        for slot in 0..SLOT_COUNT {
            if !transition_slot_allowed(kind, slot as u8) {
                state.slots[slot] = None;
                continue;
            }
            let wrote = staged[slot].is_some();
            let new_spec = if wrote {
                staged[slot].unwrap()
            } else {
                state.slots[slot].as_ref().map(|slot_state| slot_state.spec)
            };
            let Some(spec) = new_spec else {
                state.slots[slot] = None;
                continue;
            };
            let target = paint_slot(&resolved, slot);
            if spec.duration_ms == 0.0 {
                state.slots[slot] = Some(TransitionSlot {
                    spec,
                    target,
                    displayed: target,
                    anim: None,
                });
                continue;
            }
            match state.slots[slot].take() {
                None => {
                    let from = if from_batch {
                        baseline
                            .map(|style| paint_slot(&style, slot))
                            .unwrap_or(target)
                    } else {
                        target
                    };
                    let anim = if from != target {
                        Some(start_tween(spec, from, target, self.now_ms, from_batch))
                    } else {
                        None
                    };
                    state.slots[slot] = Some(TransitionSlot {
                        spec,
                        target,
                        displayed: from,
                        anim,
                    });
                }
                Some(mut existing) => {
                    if target != existing.target {
                        let from = existing.displayed;
                        let was_awaiting = existing
                            .anim
                            .as_ref()
                            .is_some_and(|anim| anim.awaiting_first_tick);
                        if from != target {
                            existing.anim = Some(start_tween(
                                spec,
                                from,
                                target,
                                self.now_ms,
                                was_awaiting || from_batch,
                            ));
                            existing.displayed = from;
                        } else {
                            existing.anim = None;
                            existing.displayed = target;
                        }
                    } else if existing.anim.is_none() {
                        existing.displayed = target;
                    }
                    existing.spec = spec;
                    existing.target = target;
                    state.slots[slot] = Some(existing);
                }
            }
        }
        self.store_transition(id, state);
        self.paint_dirty.set(true);
    }

    fn store_transition(&mut self, id: u32, state: TransitionState) {
        let any = state.slots.iter().any(Option::is_some);
        if !any {
            self.transitions.nodes.remove(&id);
            self.transitions.active.remove(&id);
            return;
        }
        let active = state
            .slots
            .iter()
            .flatten()
            .any(|slot| slot.anim.is_some());
        if active {
            self.transitions.active.insert(id);
        } else {
            self.transitions.active.remove(&id);
        }
        self.transitions.nodes.insert(id, state);
    }
}
