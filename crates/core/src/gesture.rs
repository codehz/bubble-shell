//! Pointer gestures: declarations, single/double-pointer recognition, and the
//! event batch the script thread receives.
//!
//! The UI thread owns every state in this module. Declarations arrive as ops,
//! pointer samples arrive from the host with logical coordinates and a
//! monotonic time in milliseconds. Recognition never calls into JS; it only
//! queues events and scroll requests for the caller to apply.

use std::collections::{HashMap, HashSet};

use crate::ops::Value;

/// Maximum movement that still counts as a tap.
pub const TAP_SLOP: f32 = 10.0;
/// Window that keeps a single tap waiting for a possible second tap.
pub const DOUBLE_TAP_MS: f64 = 300.0;
/// Maximum distance between the two taps of a double tap.
pub const DOUBLE_TAP_DIST: f32 = 24.0;
pub const LONG_PRESS_MS: f64 = 500.0;
pub const LONG_PRESS_DIST: f32 = 10.0;
pub const PAN_MIN: f32 = 10.0;
/// Per-sample movement below this is treated as jitter.
pub const SCROLL_SLOP: f32 = 1.0;
pub const PINCH_MIN: f64 = 0.05;
pub const ROTATE_MIN: f64 = std::f64::consts::PI / 36.0;

pub const SLOT_PRESS_START: u8 = 1;
pub const SLOT_PRESS_END: u8 = 2;
pub const SLOT_PRESS_CANCEL: u8 = 3;
pub const SLOT_TAP: u8 = 4;
pub const SLOT_DOUBLE_TAP: u8 = 5;
pub const SLOT_START: u8 = 6;
pub const SLOT_UPDATE: u8 = 7;
pub const SLOT_END: u8 = 8;
pub const SLOT_CANCEL: u8 = 9;

pub const REASON_MOVED: u8 = 1;
pub const REASON_SCROLLED: u8 = 2;
pub const REASON_SECOND_POINTER: u8 = 3;
pub const REASON_UPGRADE: u8 = 4;
pub const REASON_DISPOSED: u8 = 5;
pub const REASON_TARGET_DISPOSED: u8 = 6;
pub const REASON_HOST_CANCEL: u8 = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureKind {
    Tap = 1,
    DoubleTap = 2,
    LongPress = 3,
    Pan = 4,
    Transform = 5,
}

impl GestureKind {
    pub fn parse(value: u32) -> Option<Self> {
        Some(match value {
            1 => Self::Tap,
            2 => Self::DoubleTap,
            3 => Self::LongPress,
            4 => Self::Pan,
            5 => Self::Transform,
            _ => return None,
        })
    }

    pub fn allows_slot(self, slot: u8) -> bool {
        match self {
            Self::Tap => matches!(slot, 1..=4),
            Self::DoubleTap => matches!(slot, 1..=3 | 5),
            Self::LongPress | Self::Pan | Self::Transform => matches!(slot, 6..=9),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GestureConfig {
    pub enabled: bool,
    /// 0 = x, 1 = y, 2 = both.
    pub axis: u8,
    /// Bit 1 left, 2 right, 4 up, 8 down. Zero accepts either direction.
    pub direction: u8,
    pub min_distance: f32,
    /// `NaN` disables the cross-axis failure distance.
    pub fail_cross_distance: f32,
    /// Bit 1 x, 2 y.
    pub reserve_scroll: u8,
    pub duration: f64,
    pub max_distance: f32,
    /// Bit 1 pan, 2 pinch, 4 rotate.
    pub components: u8,
}

impl GestureConfig {
    pub fn defaults(kind: GestureKind) -> Self {
        Self {
            enabled: true,
            axis: 2,
            direction: 0,
            min_distance: match kind {
                GestureKind::LongPress => LONG_PRESS_DIST,
                _ => PAN_MIN,
            },
            fail_cross_distance: f32::NAN,
            reserve_scroll: 0,
            duration: LONG_PRESS_MS,
            max_distance: LONG_PRESS_DIST,
            components: 1,
        }
    }
}

#[derive(Clone, Debug)]
pub struct GestureDecl {
    pub id: u32,
    pub target: u32,
    pub kind: GestureKind,
    pub config: GestureConfig,
    pub events: [u32; 10],
}

impl GestureDecl {
    pub fn has_handler(&self, slot: u8) -> bool {
        self.events
            .get(slot as usize)
            .is_some_and(|handler| *handler != 0)
    }

    fn any_handler(&self, slots: &[u8]) -> bool {
        slots.iter().any(|slot| self.has_handler(*slot))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureEvent {
    pub slot: u8,
    pub gesture: u32,
    pub sequence: u32,
    pub handler: u32,
    pub target: u32,
    pub reason: u8,
    pub time_ms: f64,
    pub x: f32,
    pub y: f32,
    pub translation_x: f32,
    pub translation_y: f32,
    pub delta_x: f32,
    pub delta_y: f32,
    pub scale: f32,
    pub rotation: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
}

impl GestureEvent {
    pub fn new(time_ms: f64) -> Self {
        Self {
            slot: 0,
            gesture: 0,
            sequence: 0,
            handler: 0,
            target: 0,
            reason: 0,
            time_ms,
            x: 0.0,
            y: 0.0,
            translation_x: 0.0,
            translation_y: 0.0,
            delta_x: 0.0,
            delta_y: 0.0,
            scale: 1.0,
            rotation: 0.0,
            velocity_x: 0.0,
            velocity_y: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerAction {
    Down = 0,
    Move = 1,
    Up = 2,
    Cancel = 3,
}

impl PointerAction {
    pub fn parse(value: u32) -> Option<Self> {
        Some(match value {
            0 => Self::Down,
            1 => Self::Move,
            2 => Self::Up,
            3 => Self::Cancel,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct PointerInput {
    pub action: PointerAction,
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub time_ms: f64,
    pub buttons: u32,
    /// 0 touch, 1 mouse, 2 pen, other unknown.
    pub pointer_kind: u8,
    /// Real hit path (deepest first) for `Down`; empty otherwise.
    pub path: Vec<u32>,
}

impl PointerInput {
    /// Mouse input only starts a sequence with the primary button.
    fn eligible(&self) -> bool {
        self.pointer_kind != 1 || self.buttons & 1 != 0
    }
}

#[derive(Clone, Debug)]
pub struct ScrollRequest {
    pub path: Vec<u32>,
    pub dx: f32,
    pub dy: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CandState {
    Pending,
    Accepted,
    Failed,
}

#[derive(Clone, Debug)]
struct Cand {
    decl: u32,
    kind: GestureKind,
    state: CandState,
    start_x: f32,
    start_y: f32,
    last_x: f32,
    last_y: f32,
    last_t: f64,
    velocity_x: f32,
    velocity_y: f32,
}

#[derive(Clone, Debug)]
struct Pointer {
    id: u32,
    x: f32,
    y: f32,
}

#[derive(Clone, Debug)]
struct TapFamily {
    target: u32,
    tap: Option<u32>,
    double: Option<u32>,
}

#[derive(Clone, Debug)]
struct PendingTap {
    target: u32,
    x: f32,
    y: f32,
    time_ms: f64,
    deadline_ms: f64,
    gesture: u32,
    tap_handler: u32,
    sequence: u32,
}

#[derive(Clone, Debug)]
struct TransformState {
    base_x: f32,
    base_y: f32,
    base_distance: f32,
    base_angle: f64,
    last_x: f32,
    last_y: f32,
    last_t: f64,
    translation_x: f32,
    translation_y: f32,
    scale: f32,
    rotation: f64,
    velocity_x: f32,
    velocity_y: f32,
}

#[derive(Clone, Debug)]
struct Sequence {
    id: u32,
    path: Vec<u32>,
    active: Vec<u32>,
    pair: Vec<Pointer>,
    cands: Vec<Cand>,
    tap: Option<TapFamily>,
    /// A second tap that may complete a double tap.
    second: Option<PendingTap>,
    start_x: f32,
    start_y: f32,
    start_t: f64,
    last_x: f32,
    last_y: f32,
    last_t: f64,
    press_active: bool,
    tap_failed: bool,
    scroll_used: bool,
    axes_owned: u8,
    reserved: u8,
    buffer: [f32; 2],
    scroll_last_x: f32,
    scroll_last_y: f32,
    transform: Option<TransformState>,
    transform_decl: Option<u32>,
    dead: bool,
}

/// Pointer declarations plus the one live touch sequence.
pub struct Recognizer {
    decls: Vec<Option<GestureDecl>>,
    order: HashMap<u32, Vec<u32>>,
    retired: HashSet<u32>,
    sequence: Option<Sequence>,
    pending_tap: Option<PendingTap>,
    next_sequence: u32,
    events: Vec<GestureEvent>,
    scroll_out: Vec<ScrollRequest>,
}

impl Default for Recognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Recognizer {
    pub fn new() -> Self {
        Self {
            decls: Vec::new(),
            order: HashMap::new(),
            retired: HashSet::new(),
            sequence: None,
            pending_tap: None,
            next_sequence: 0,
            events: Vec::new(),
            scroll_out: Vec::new(),
        }
    }

    pub fn spawn(
        &mut self,
        id: u32,
        target: u32,
        kind: GestureKind,
        index: u32,
    ) -> Result<(), String> {
        if id == 0 || self.retired.contains(&id) || self.decl(id).is_some() {
            return Err(format!("gesture {id} is not free"));
        }
        let list = self.order.entry(target).or_default();
        if index as usize > list.len() {
            return Err(format!(
                "gesture index {index} past {} declarations",
                list.len()
            ));
        }
        list.insert(index as usize, id);
        if self.decls.len() <= id as usize {
            self.decls.resize_with(id as usize + 1, || None);
        }
        self.decls[id as usize] = Some(GestureDecl {
            id,
            target,
            kind,
            config: GestureConfig::defaults(kind),
            events: [0; 10],
        });
        Ok(())
    }

    pub fn decl(&self, id: u32) -> Option<&GestureDecl> {
        self.decls.get(id as usize).and_then(Option::as_ref)
    }

    pub fn set_prop(&mut self, id: u32, key: &str, value: &Value) -> Result<(), String> {
        let decl = self
            .decls
            .get_mut(id as usize)
            .and_then(Option::as_mut)
            .ok_or_else(|| format!("unknown gesture {id}"))?;
        let kind = decl.kind;
        let config = &mut decl.config;
        match (kind, key) {
            (_, "enabled") => config.enabled = value.as_bool()?,
            (_, "reserveScroll") => config.reserve_scroll = scroll_bits(value)?,
            (GestureKind::Pan, "axis") => config.axis = axis(value)?,
            (GestureKind::Pan, "direction") => config.direction = direction(value)?,
            (GestureKind::Pan | GestureKind::Transform, "minDistance") => {
                config.min_distance = non_negative(value)?
            }
            (GestureKind::Pan, "failCrossDistance") => {
                config.fail_cross_distance = match value.optional_f32()? {
                    Some(distance) if distance >= 0.0 => distance,
                    Some(_) => return Err("failCrossDistance must not be negative".into()),
                    None => f32::NAN,
                };
            }
            (GestureKind::LongPress, "duration") => {
                config.duration = value.as_f32()?.max(0.0) as f64
            }
            (GestureKind::LongPress, "maxDistance") => config.max_distance = non_negative(value)?,
            (GestureKind::Transform, "pan") => set_component(config, 1, value.as_bool()?),
            (GestureKind::Transform, "pinch") => set_component(config, 2, value.as_bool()?),
            (GestureKind::Transform, "rotate") => set_component(config, 4, value.as_bool()?),
            _ => return Err(format!("gesture {kind:?} has no property {key}")),
        }
        Ok(())
    }

    pub fn bind(&mut self, id: u32, slot: u8, handler: u32) -> Result<(), String> {
        let decl = self
            .decls
            .get_mut(id as usize)
            .and_then(Option::as_mut)
            .ok_or_else(|| format!("unknown gesture {id}"))?;
        if !decl.kind.allows_slot(slot) {
            return Err(format!("gesture {:?} has no event slot {slot}", decl.kind));
        }
        decl.events[slot as usize] = handler;
        Ok(())
    }

    pub fn move_decl(&mut self, id: u32, target: u32, index: u32) -> Result<(), String> {
        let old_target = self
            .decl(id)
            .ok_or_else(|| format!("unknown gesture {id}"))?
            .target;
        if let Some(list) = self.order.get_mut(&old_target) {
            list.retain(|existing| *existing != id);
        }
        {
            let list = self.order.entry(target).or_default();
            if index as usize > list.len() {
                return Err(format!(
                    "gesture index {index} past {} declarations",
                    list.len()
                ));
            }
            list.insert(index as usize, id);
        }
        if old_target != target {
            self.cancel_decl(id, REASON_DISPOSED);
            if let Some(decl) = self
                .decls
                .get_mut(id as usize)
                .and_then(Option::as_mut)
            {
                decl.target = target;
            }
        }
        Ok(())
    }

    pub fn dispose(&mut self, id: u32) -> Result<(), String> {
        let target = self
            .decl(id)
            .ok_or_else(|| format!("unknown gesture {id}"))?
            .target;
        self.cancel_decl(id, REASON_DISPOSED);
        if let Some(list) = self.order.get_mut(&target) {
            list.retain(|existing| *existing != id);
            if list.is_empty() {
                self.order.remove(&target);
            }
        }
        if self.decls.len() > id as usize {
            self.decls[id as usize] = None;
        }
        self.retired.insert(id);
        Ok(())
    }

    /// Drop every declaration attached to a destroyed node.
    pub fn target_disposed(&mut self, target: u32) {
        let ids: Vec<u32> = self.order.get(&target).cloned().unwrap_or_default();
        for id in ids {
            let before = self.events.len();
            let _ = self.dispose(id);
            if let Some(event) = self.events[before..].iter_mut().find(|event| {
                event.slot == SLOT_CANCEL && event.reason == REASON_DISPOSED
            }) {
                event.reason = REASON_TARGET_DISPOSED;
            }
        }
        self.order.remove(&target);
    }

    pub fn drain_events(&mut self) -> Vec<GestureEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn drain_scroll(&mut self) -> Vec<ScrollRequest> {
        std::mem::take(&mut self.scroll_out)
    }

    pub fn active(&self) -> bool {
        self.sequence.is_some()
    }

    pub fn pointer(&mut self, input: PointerInput) {
        match input.action {
            PointerAction::Down => self.down(input),
            PointerAction::Move => self.move_pointer(input),
            PointerAction::Up => self.up(input),
            PointerAction::Cancel => self.cancel_sequence(REASON_HOST_CANCEL),
        }
    }

    /// Advance timers that do not need a pointer sample.
    pub fn advance(&mut self, now_ms: f64) {
        if let Some(pending) = &self.pending_tap {
            if now_ms >= pending.deadline_ms {
                let pending = self.pending_tap.take().expect("pending tap");
                emit_tap(&mut self.events, &pending);
            }
        }
        let Some(seq) = &mut self.sequence else {
            return;
        };
        let Self { decls, events, .. } = self;
        let mut accepts = Vec::new();
        for (index, cand) in seq.cands.iter().enumerate() {
            if cand.kind != GestureKind::LongPress || cand.state != CandState::Pending {
                continue;
            }
            let Some(Some(decl)) = decls.get(cand.decl as usize) else {
                continue;
            };
            if !decl.config.enabled {
                continue;
            }
            let dx = seq.last_x - cand.start_x;
            let dy = seq.last_y - cand.start_y;
            if dx.hypot(dy) > decl.config.max_distance {
                continue;
            }
            if now_ms - seq.start_t >= decl.config.duration {
                accepts.push(index);
            }
        }
        for index in accepts {
            if let Some(cand) = seq.cands.get_mut(index) {
                if cand.state == CandState::Pending {
                    cand.state = CandState::Accepted;
                    cand.last_t = now_ms;
                }
            }
            let id = seq.cands[index].decl;
            let (x, y) = (seq.last_x, seq.last_y);
            let sequence = seq.id;
            let target = decls
                .get(id as usize)
                .and_then(Option::as_ref)
                .map(|decl| decl.target)
                .unwrap_or(0);
            push_event(events, decls, id, SLOT_START, sequence, now_ms, |event| {
                event.target = target;
                event.x = x;
                event.y = y;
            });
            seq.axes_owned |= 3;
        }
        let (x, y) = (seq.last_x, seq.last_y);
        self.update_reservation();
        self.fail_tap(REASON_UPGRADE, now_ms, x, y);
    }

    fn down(&mut self, input: PointerInput) {
        if !input.eligible() {
            return;
        }
        if let Some(seq) = &mut self.sequence {
            if seq.active.contains(&input.id) {
                return;
            }
            seq.active.push(input.id);
            if !seq.dead && seq.pair.len() == 1 && seq.transform_decl.is_none() {
                self.begin_second(input);
            }
            return;
        }
        if input.path.is_empty() {
            return;
        }
        self.next_sequence = self.next_sequence.wrapping_add(1);
        let sequence = self.next_sequence;
        let Self {
            decls,
            order,
            sequence: slot,
            pending_tap,
            events,
            ..
        } = self;
        let mut cands = Vec::new();
        for target in &input.path {
            let Some(ids) = order.get(target) else {
                continue;
            };
            for id in ids {
                let Some(decl) = decls.get(*id as usize).and_then(Option::as_ref) else {
                    continue;
                };
                if !decl.config.enabled
                    || !matches!(decl.kind, GestureKind::Pan | GestureKind::LongPress)
                {
                    continue;
                }
                if !decl.any_handler(&[SLOT_START, SLOT_UPDATE, SLOT_END, SLOT_CANCEL]) {
                    continue;
                }
                cands.push(Cand {
                    decl: *id,
                    kind: decl.kind,
                    state: CandState::Pending,
                    start_x: input.x,
                    start_y: input.y,
                    last_x: input.x,
                    last_y: input.y,
                    last_t: input.time_ms,
                    velocity_x: 0.0,
                    velocity_y: 0.0,
                });
            }
        }
        let tap = tap_family(order, decls, &input.path);
        let mut seq = Sequence {
            id: sequence,
            path: input.path.clone(),
            active: vec![input.id],
            pair: vec![Pointer {
                id: input.id,
                x: input.x,
                y: input.y,
            }],
            cands,
            tap: tap.clone(),
            second: None,
            start_x: input.x,
            start_y: input.y,
            start_t: input.time_ms,
            last_x: input.x,
            last_y: input.y,
            last_t: input.time_ms,
            press_active: false,
            tap_failed: false,
            scroll_used: false,
            axes_owned: 0,
            reserved: 0,
            buffer: [0.0; 2],
            scroll_last_x: input.x,
            scroll_last_y: input.y,
            transform: None,
            transform_decl: None,
            dead: false,
        };
        if let Some(pending) = pending_tap.as_ref() {
            let matches = tap.as_ref().is_some_and(|family| {
                family.target == pending.target
                    && (pending.x - input.x).hypot(pending.y - input.y) <= DOUBLE_TAP_DIST
                    && input.time_ms - pending.time_ms <= DOUBLE_TAP_MS
            });
            if matches {
                seq.second = pending_tap.take();
            }
        }
        if let Some(family) = &tap {
            if let Some(id) = family.tap {
                if decls
                    .get(id as usize)
                    .and_then(Option::as_ref)
                    .is_some_and(|decl| decl.has_handler(SLOT_PRESS_START))
                {
                    seq.press_active = true;
                    let target = family.target;
                    push_event(events, decls, id, SLOT_PRESS_START, sequence, input.time_ms, |event| {
                        event.target = target;
                        event.x = input.x;
                        event.y = input.y;
                    });
                }
            }
        }
        *slot = Some(seq);
        self.update_reservation();
    }

    fn begin_second(&mut self, input: PointerInput) {
        let Self {
            decls,
            order,
            sequence,
            events,
            ..
        } = self;
        let Some(seq) = sequence.as_mut() else {
            return;
        };
        let time = input.time_ms;
        if seq.press_active {
            if let Some(id) = seq.tap.as_ref().and_then(|family| family.tap) {
                let target = seq.tap.as_ref().map(|family| family.target).unwrap_or(0);
                push_event(events, decls, id, SLOT_PRESS_CANCEL, seq.id, time, |event| {
                    event.target = target;
                    event.x = input.x;
                    event.y = input.y;
                    event.reason = REASON_SECOND_POINTER;
                });
            }
            seq.press_active = false;
        }
        if let Some(pending) = seq.second.take() {
            emit_tap(events, &pending);
        }
        seq.tap_failed = true;
        for cand in &mut seq.cands {
            if cand.kind == GestureKind::LongPress && cand.state == CandState::Pending {
                cand.state = CandState::Failed;
            }
        }
        let first = seq.pair[0].clone();
        seq.pair.push(Pointer {
            id: input.id,
            x: input.x,
            y: input.y,
        });
        let second_path: HashSet<u32> = input.path.iter().copied().collect();
        let mut transform_cands = Vec::new();
        for target in &seq.path {
            if !second_path.contains(target) {
                continue;
            }
            let Some(ids) = order.get(target) else {
                continue;
            };
            for id in ids {
                let Some(decl) = decls.get(*id as usize).and_then(Option::as_ref) else {
                    continue;
                };
                if decl.kind != GestureKind::Transform || !decl.config.enabled {
                    continue;
                }
                if !decl.any_handler(&[SLOT_START, SLOT_UPDATE, SLOT_END, SLOT_CANCEL]) {
                    continue;
                }
                transform_cands.push(Cand {
                    decl: *id,
                    kind: GestureKind::Transform,
                    state: CandState::Pending,
                    start_x: input.x,
                    start_y: input.y,
                    last_x: input.x,
                    last_y: input.y,
                    last_t: time,
                    velocity_x: 0.0,
                    velocity_y: 0.0,
                });
            }
        }
        seq.cands.retain(|cand| cand.kind != GestureKind::Transform);
        seq.cands.extend(transform_cands);
        let cx = (first.x + input.x) / 2.0;
        let cy = (first.y + input.y) / 2.0;
        let dx = input.x - first.x;
        let dy = input.y - first.y;
        seq.transform = Some(TransformState {
            base_x: cx,
            base_y: cy,
            base_distance: dx.hypot(dy),
            base_angle: (dy as f64).atan2(dx as f64),
            last_x: cx,
            last_y: cy,
            last_t: time,
            translation_x: 0.0,
            translation_y: 0.0,
            scale: 1.0,
            rotation: 0.0,
            velocity_x: 0.0,
            velocity_y: 0.0,
        });
        self.update_reservation();
    }

    fn move_pointer(&mut self, input: PointerInput) {
        {
            let Some(seq) = &mut self.sequence else {
                return;
            };
            let Some(index) = seq.pair.iter().position(|pointer| pointer.id == input.id) else {
                return;
            };
            if seq.dead {
                return;
            }
            let pointer = &mut seq.pair[index];
            pointer.x = input.x;
            pointer.y = input.y;
        }
        if self
            .sequence
            .as_ref()
            .is_some_and(|seq| seq.transform_decl.is_some())
        {
            self.update_transform(input.time_ms);
            return;
        }
        if self.sequence.as_ref().is_some_and(|seq| seq.pair.len() == 2) {
            self.evaluate_transform(input.time_ms);
            if self
                .sequence
                .as_ref()
                .is_some_and(|seq| seq.transform_decl.is_some())
            {
                self.update_transform(input.time_ms);
                return;
            }
        }
        let Some(seq) = &mut self.sequence else {
            return;
        };
        let (x, y) = (seq.pair[0].x, seq.pair[0].y);
        seq.last_x = x;
        seq.last_y = y;
        seq.last_t = input.time_ms;
        self.evaluate_single(input.time_ms);
        self.scroll_step(input.time_ms);
    }

    fn evaluate_single(&mut self, time_ms: f64) {
        let Self {
            decls,
            sequence,
            events,
            ..
        } = self;
        let Some(seq) = sequence.as_mut() else {
            return;
        };
        let (x, y) = (seq.last_x, seq.last_y);
        let mut fail_tap = false;
        let mut accepted = Vec::new();
        for (index, cand) in seq.cands.iter_mut().enumerate() {
            if cand.state != CandState::Pending {
                continue;
            }
            let Some(Some(decl)) = decls.get(cand.decl as usize) else {
                cand.state = CandState::Failed;
                continue;
            };
            let config = &decl.config;
            if !config.enabled {
                cand.state = CandState::Failed;
                continue;
            }
            let dx = x - cand.start_x;
            let dy = y - cand.start_y;
            match cand.kind {
                GestureKind::Pan => {
                    let (main, cross, bit) = match config.axis {
                        0 => (dx, dy, 1),
                        1 => (dy, dx, 2),
                        _ => (dx.hypot(dy), 0.0, 3),
                    };
                    if config.direction != 0 && !direction_matches(config.direction, config.axis, dx, dy)
                    {
                        cand.state = CandState::Failed;
                        continue;
                    }
                    if config.fail_cross_distance.is_finite()
                        && config.axis != 2
                        && cross.abs() > config.fail_cross_distance
                    {
                        cand.state = CandState::Failed;
                        continue;
                    }
                    if main.abs() < config.min_distance {
                        continue;
                    }
                    if seq.axes_owned & bit != 0 {
                        cand.state = CandState::Failed;
                        continue;
                    }
                    let dt = (time_ms - cand.last_t).max(1.0) as f32;
                    cand.velocity_x = (x - cand.last_x) / dt * 1000.0;
                    cand.velocity_y = (y - cand.last_y) / dt * 1000.0;
                    cand.state = CandState::Accepted;
                    seq.axes_owned |= bit;
                    accepted.push(index);
                }
                GestureKind::LongPress => {
                    if dx.hypot(dy) > config.max_distance {
                        cand.state = CandState::Failed;
                    }
                }
                GestureKind::Transform | GestureKind::Tap | GestureKind::DoubleTap => {}
            }
        }
        if (x - seq.start_x).hypot(y - seq.start_y) > TAP_SLOP {
            fail_tap = true;
        }
        for index in accepted {
            let cand = &mut seq.cands[index];
            let id = cand.decl;
            let (start_x, start_y) = (cand.start_x, cand.start_y);
            let (vx, vy) = (cand.velocity_x, cand.velocity_y);
            let target = decls
                .get(id as usize)
                .and_then(Option::as_ref)
                .map(|decl| decl.target)
                .unwrap_or(0);
            push_event(events, decls, id, SLOT_START, seq.id, time_ms, |event| {
                event.target = target;
                event.x = x;
                event.y = y;
                event.translation_x = x - start_x;
                event.translation_y = y - start_y;
                event.velocity_x = vx;
                event.velocity_y = vy;
            });
        }
        let won = seq.cands.iter().any(|cand| cand.state == CandState::Accepted);
        self.update_reservation();
        // Any winner invalidates the click family.
        if won {
            self.fail_tap(REASON_UPGRADE, time_ms, x, y);
        } else if fail_tap {
            self.fail_tap(REASON_MOVED, time_ms, x, y);
        }
    }

    fn evaluate_transform(&mut self, time_ms: f64) {
        let Self {
            decls,
            sequence,
            events,
            ..
        } = self;
        let Some(seq) = sequence.as_mut() else {
            return;
        };
        if seq.pair.len() != 2 {
            return;
        }
        let Some(state) = &seq.transform else {
            return;
        };
        let (x0, y0) = (seq.pair[0].x, seq.pair[0].y);
        let (x1, y1) = (seq.pair[1].x, seq.pair[1].y);
        let cx = (x0 + x1) / 2.0;
        let cy = (y0 + y1) / 2.0;
        let distance = (x1 - x0).hypot(y1 - y0);
        let angle = ((y1 - y0) as f64).atan2((x1 - x0) as f64);
        let scale = if state.base_distance > 0.001 {
            (distance / state.base_distance) as f64
        } else {
            1.0
        };
        let rotation = wrap_angle(angle - state.base_angle);
        let translation = (cx - state.base_x).hypot(cy - state.base_y);
        let mut matched = None;
        for cand in &seq.cands {
            if cand.kind != GestureKind::Transform || cand.state != CandState::Pending {
                continue;
            }
            let Some(Some(decl)) = decls.get(cand.decl as usize) else {
                continue;
            };
            let components = if decl.config.components == 0 {
                1
            } else {
                decl.config.components
            };
            let hit = (components & 1 != 0 && translation >= decl.config.min_distance)
                || (components & 2 != 0 && (scale - 1.0).abs() >= PINCH_MIN)
                || (components & 4 != 0 && rotation.abs() >= ROTATE_MIN);
            if hit {
                matched = Some(cand.decl);
                break;
            }
        }
        let Some(id) = matched else {
            return;
        };
        // The transform overrides single-pointer ownership of both axes.
        let panned: Vec<u32> = seq
            .cands
            .iter()
            .filter(|cand| {
                cand.state == CandState::Accepted && cand.kind != GestureKind::Transform
            })
            .map(|cand| cand.decl)
            .collect();
        for decl in panned {
            let target = decls
                .get(decl as usize)
                .and_then(Option::as_ref)
                .map(|decl| decl.target)
                .unwrap_or(0);
            push_event(events, decls, decl, SLOT_CANCEL, seq.id, time_ms, |event| {
                event.target = target;
                event.reason = REASON_UPGRADE;
            });
        }
        for cand in &mut seq.cands {
            if cand.decl != id && cand.state == CandState::Pending {
                cand.state = CandState::Failed;
            }
        }
        seq.axes_owned = 3;
        seq.reserved = 0;
        seq.buffer = [0.0; 2];
        seq.transform_decl = Some(id);
        seq.tap_failed = true;
        let target = decls
            .get(id as usize)
            .and_then(Option::as_ref)
            .map(|decl| decl.target)
            .unwrap_or(0);
        push_event(events, decls, id, SLOT_START, seq.id, time_ms, |event| {
            event.target = target;
            event.x = cx;
            event.y = cy;
        });
        self.fail_tap(REASON_UPGRADE, time_ms, cx, cy);
    }

    fn scroll_step(&mut self, time_ms: f64) {
        let Self {
            decls,
            sequence,
            scroll_out,
            ..
        } = self;
        let Some(seq) = sequence.as_mut() else {
            return;
        };
        if seq.dead || seq.transform_decl.is_some() {
            return;
        }
        let (x, y) = (seq.pair[0].x, seq.pair[0].y);
        let dx = x - seq.scroll_last_x;
        let dy = y - seq.scroll_last_y;
        seq.scroll_last_x = x;
        seq.scroll_last_y = y;
        let mut requests: Vec<ScrollRequest> = Vec::new();
        let mut scroll_used = false;
        let mut fail_indices: Vec<usize> = Vec::new();
        for axis in 0..2 {
            let delta = if axis == 0 { dx } else { dy };
            if delta.abs() < SCROLL_SLOP {
                continue;
            }
            let bit = 1u8 << axis;
            if seq.axes_owned & bit != 0 {
                seq.buffer[axis] = 0.0;
                continue;
            }
            if seq.reserved & bit != 0 {
                seq.buffer[axis] += delta;
                continue;
            }
            // Scrolling starting on an axis beats a still-pending pan.
            for (index, cand) in seq.cands.iter().enumerate() {
                if cand.state != CandState::Pending || cand.kind != GestureKind::Pan {
                    continue;
                }
                let Some(Some(decl)) = decls.get(cand.decl as usize) else {
                    continue;
                };
                let axes = match decl.config.axis {
                    0 => 1,
                    1 => 2,
                    _ => 3,
                };
                if axes & bit != 0 {
                    fail_indices.push(index);
                }
            }
            let amount = -(delta + seq.buffer[axis]);
            seq.buffer[axis] = 0.0;
            let (rdx, rdy) = if axis == 0 {
                (amount, 0.0)
            } else {
                (0.0, amount)
            };
            requests.push(ScrollRequest {
                path: seq.path.clone(),
                dx: rdx,
                dy: rdy,
            });
            scroll_used = true;
        }
        for index in fail_indices {
            if let Some(cand) = seq.cands.get_mut(index) {
                if cand.state == CandState::Pending {
                    cand.state = CandState::Failed;
                }
            }
        }
        if scroll_used {
            seq.scroll_used = true;
            scroll_out.extend(requests);
        }
        self.update_reservation();
        if scroll_used {
            self.fail_tap(REASON_SCROLLED, time_ms, x, y);
        }
    }

    fn update_transform(&mut self, time_ms: f64) {
        let Self {
            decls,
            sequence,
            events,
            ..
        } = self;
        let Some(seq) = sequence.as_mut() else {
            return;
        };
        let Some(id) = seq.transform_decl else {
            return;
        };
        let Some(state) = seq.transform.as_mut() else {
            return;
        };
        let (x0, y0) = (seq.pair[0].x, seq.pair[0].y);
        let (x1, y1) = (seq.pair[1].x, seq.pair[1].y);
        let cx = (x0 + x1) / 2.0;
        let cy = (y0 + y1) / 2.0;
        let distance = (x1 - x0).hypot(y1 - y0);
        let angle = ((y1 - y0) as f64).atan2((x1 - x0) as f64);
        let scale = if state.base_distance > 0.001 {
            (distance / state.base_distance) as f32
        } else {
            1.0
        };
        let target_rotation = angle - state.base_angle;
        state.rotation += wrap_offset(state.rotation, target_rotation);
        let translation_x = cx - state.base_x;
        let translation_y = cy - state.base_y;
        let dt = (time_ms - state.last_t).max(1.0) as f32;
        state.velocity_x = (cx - state.last_x) / dt * 1000.0;
        state.velocity_y = (cy - state.last_y) / dt * 1000.0;
        let delta_x = cx - state.last_x;
        let delta_y = cy - state.last_y;
        state.last_x = cx;
        state.last_y = cy;
        state.last_t = time_ms;
        state.translation_x = translation_x;
        state.translation_y = translation_y;
        state.scale = scale;
        let rotation = state.rotation;
        let (vx, vy) = (state.velocity_x, state.velocity_y);
        let target = decls
            .get(id as usize)
            .and_then(Option::as_ref)
            .map(|decl| decl.target)
            .unwrap_or(0);
        push_event(events, decls, id, SLOT_UPDATE, seq.id, time_ms, |event| {
            event.target = target;
            event.x = cx;
            event.y = cy;
            event.translation_x = translation_x;
            event.translation_y = translation_y;
            event.delta_x = delta_x;
            event.delta_y = delta_y;
            event.scale = scale;
            event.rotation = rotation as f32;
            event.velocity_x = vx;
            event.velocity_y = vy;
        });
    }

    fn up(&mut self, input: PointerInput) {
        let Some(seq) = &mut self.sequence else {
            return;
        };
        {
            if let Some(pointer) = seq.pair.iter_mut().find(|pointer| pointer.id == input.id) {
                pointer.x = input.x;
                pointer.y = input.y;
            }
        }
        seq.last_x = input.x;
        seq.last_y = input.y;
        seq.last_t = input.time_ms;
        let in_pair = seq.pair.iter().any(|pointer| pointer.id == input.id);
        if seq.transform_decl.is_some() {
            if in_pair {
                self.end_transform(input.time_ms);
                if let Some(seq) = &mut self.sequence {
                    seq.dead = true;
                }
            }
        } else if seq.pair.len() == 2 {
            if in_pair {
                seq.pair.retain(|pointer| pointer.id != input.id);
                seq.dead = true;
            }
        } else if seq.pair.first().is_some_and(|pointer| pointer.id == input.id) {
            self.finish_single(input.time_ms, input.x, input.y);
            if let Some(seq) = &mut self.sequence {
                seq.dead = true;
            }
        }
        if let Some(seq) = &mut self.sequence {
            seq.active.retain(|id| *id != input.id);
            if seq.active.is_empty() {
                self.sequence = None;
                self.update_reservation();
            }
        }
    }

    fn finish_single(&mut self, time_ms: f64, x: f32, y: f32) {
        let Self {
            decls,
            sequence,
            events,
            pending_tap,
            ..
        } = self;
        let Some(seq) = sequence.as_mut() else {
            return;
        };
        let accepted: Vec<(u32, f32, f32, f32, f32, f32, f32)> = seq
            .cands
            .iter()
            .filter(|cand| cand.state == CandState::Accepted)
            .map(|cand| {
                (
                    cand.decl,
                    cand.last_x,
                    cand.last_y,
                    cand.velocity_x,
                    cand.velocity_y,
                    cand.start_x,
                    cand.start_y,
                )
            })
            .collect();
        for (id, last_x, last_y, vx, vy, start_x, start_y) in accepted {
            let target = decls
                .get(id as usize)
                .and_then(Option::as_ref)
                .map(|decl| decl.target)
                .unwrap_or(0);
            push_event(events, decls, id, SLOT_END, seq.id, time_ms, |event| {
                event.target = target;
                event.x = x;
                event.y = y;
                event.translation_x = x - start_x;
                event.translation_y = y - start_y;
                event.delta_x = x - last_x;
                event.delta_y = y - last_y;
                event.velocity_x = vx;
                event.velocity_y = vy;
            });
        }
        let tap_ok = !seq.tap_failed
            && !seq.scroll_used
            && (x - seq.start_x).hypot(y - seq.start_y) <= TAP_SLOP
            && !seq
                .cands
                .iter()
                .any(|cand| cand.state == CandState::Accepted);
        if !tap_ok {
            return;
        }
        let Some(family) = seq.tap.clone() else {
            return;
        };
        let sequence = seq.id;
        if seq.press_active {
            if let Some(id) = family.tap {
                seq.press_active = false;
                let target = family.target;
                push_event(events, decls, id, SLOT_PRESS_END, sequence, time_ms, |event| {
                    event.target = target;
                    event.x = x;
                    event.y = y;
                });
            }
        }
        if seq.second.take().is_some() {
            if let Some(id) = family.double {
                push_event(events, decls, id, SLOT_DOUBLE_TAP, sequence, time_ms, |event| {
                    event.target = family.target;
                    event.x = x;
                    event.y = y;
                });
            }
            return;
        }
        let tap_handler = family
            .tap
            .and_then(|id| decls.get(id as usize).and_then(Option::as_ref))
            .map(|decl| decl.events[SLOT_TAP as usize])
            .unwrap_or(0);
        let double_wait = family
            .double
            .and_then(|id| decls.get(id as usize).and_then(Option::as_ref))
            .is_some_and(|decl| decl.has_handler(SLOT_DOUBLE_TAP));
        let pending = PendingTap {
            target: family.target,
            x,
            y,
            time_ms,
            deadline_ms: time_ms + DOUBLE_TAP_MS,
            gesture: family.tap.unwrap_or(0),
            tap_handler,
            sequence,
        };
        if double_wait {
            *pending_tap = Some(pending);
        } else {
            emit_tap(events, &pending);
        }
    }

    fn end_transform(&mut self, time_ms: f64) {
        let Self {
            decls,
            sequence,
            events,
            ..
        } = self;
        let Some(seq) = sequence.as_mut() else {
            return;
        };
        let Some(id) = seq.transform_decl else {
            return;
        };
        let Some(state) = seq.transform.as_ref() else {
            return;
        };
        let (x, y) = (state.last_x, state.last_y);
        let (tx, ty, scale, rotation) = (
            state.translation_x,
            state.translation_y,
            state.scale,
            state.rotation,
        );
        let (vx, vy) = (state.velocity_x, state.velocity_y);
        let target = decls
            .get(id as usize)
            .and_then(Option::as_ref)
            .map(|decl| decl.target)
            .unwrap_or(0);
        push_event(events, decls, id, SLOT_END, seq.id, time_ms, |event| {
            event.target = target;
            event.x = x;
            event.y = y;
            event.translation_x = tx;
            event.translation_y = ty;
            event.scale = scale;
            event.rotation = rotation as f32;
            event.velocity_x = vx;
            event.velocity_y = vy;
        });
    }

    fn fail_tap(&mut self, reason: u8, time_ms: f64, x: f32, y: f32) {
        let Self {
            decls,
            sequence,
            events,
            ..
        } = self;
        let Some(seq) = sequence.as_mut() else {
            return;
        };
        if seq.tap_failed {
            return;
        }
        seq.tap_failed = true;
        let tap = seq.tap.as_ref().and_then(|family| family.tap);
        let target = seq.tap.as_ref().map(|family| family.target).unwrap_or(0);
        let press = seq.press_active;
        seq.press_active = false;
        let pending = seq.second.take();
        if press {
            if let Some(id) = tap {
                push_event(events, decls, id, SLOT_PRESS_CANCEL, seq.id, time_ms, |event| {
                    event.target = target;
                    event.x = x;
                    event.y = y;
                    event.reason = reason;
                });
            }
        }
        if let Some(pending) = pending {
            emit_tap(events, &pending);
        }
    }

    fn cancel_decl(&mut self, id: u32, reason: u8) {
        let Self {
            decls,
            sequence,
            events,
            ..
        } = self;
        let Some(seq) = sequence.as_mut() else {
            return;
        };
        let time = seq.last_t;
        let (x, y) = (seq.last_x, seq.last_y);
        let mut canceled = false;
        if let Some(cand) = seq.cands.iter_mut().find(|cand| cand.decl == id) {
            if cand.state == CandState::Accepted {
                canceled = true;
            }
            cand.state = CandState::Failed;
        }
        if canceled {
            let target = decls
                .get(id as usize)
                .and_then(Option::as_ref)
                .map(|decl| decl.target)
                .unwrap_or(0);
            push_event(events, decls, id, SLOT_CANCEL, seq.id, time, |event| {
                event.target = target;
                event.x = x;
                event.y = y;
                event.reason = reason;
            });
        }
        let mut press_cancel = false;
        if seq.tap.as_ref().is_some_and(|family| {
            family.tap == Some(id) || family.double == Some(id)
        }) {
            press_cancel = seq.press_active;
            seq.tap_failed = true;
        }
        if seq.transform_decl == Some(id) {
            seq.transform_decl = None;
            seq.dead = true;
            let target = decls
                .get(id as usize)
                .and_then(Option::as_ref)
                .map(|decl| decl.target)
                .unwrap_or(0);
            push_event(events, decls, id, SLOT_CANCEL, seq.id, time, |event| {
                event.target = target;
                event.reason = reason;
            });
        }
        if press_cancel {
            seq.press_active = false;
            if let Some(tap) = seq.tap.as_ref().and_then(|family| family.tap) {
                let target = seq.tap.as_ref().map(|family| family.target).unwrap_or(0);
                push_event(events, decls, tap, SLOT_PRESS_CANCEL, seq.id, time, |event| {
                    event.target = target;
                    event.x = x;
                    event.y = y;
                    event.reason = reason;
                });
            }
        }
        self.update_reservation();
    }

    fn cancel_sequence(&mut self, reason: u8) {
        let Some(seq) = self.sequence.take() else {
            return;
        };
        let time = seq.last_t;
        let (x, y) = (seq.last_x, seq.last_y);
        let accepted: Vec<u32> = seq
            .cands
            .iter()
            .filter(|cand| cand.state == CandState::Accepted)
            .map(|cand| cand.decl)
            .collect();
        for id in accepted {
            let target = self.decl(id).map(|decl| decl.target).unwrap_or(0);
            emit_for(
                &mut self.events,
                &self.decls,
                id,
                SLOT_CANCEL,
                seq.id,
                time,
                x,
                y,
                reason,
                target,
            );
        }
        if seq.press_active {
            if let Some(id) = seq.tap.as_ref().and_then(|family| family.tap) {
                emit_for(
                    &mut self.events,
                    &self.decls,
                    id,
                    SLOT_PRESS_CANCEL,
                    seq.id,
                    time,
                    x,
                    y,
                    reason,
                    0,
                );
            }
        }
    }

    fn update_reservation(&mut self) {
        let Self {
            decls,
            sequence,
            scroll_out,
            ..
        } = self;
        let Some(seq) = sequence.as_mut() else {
            return;
        };
        let mut reserved = 0u8;
        for cand in &seq.cands {
            if cand.state != CandState::Pending {
                continue;
            }
            if let Some(Some(decl)) = decls.get(cand.decl as usize) {
                if decl.config.enabled {
                    reserved |= decl.config.reserve_scroll;
                }
            }
        }
        let released = seq.reserved & !reserved;
        seq.reserved = reserved;
        for axis in 0..2 {
            let bit = 1u8 << axis;
            if released & bit == 0 {
                continue;
            }
            let buffered = seq.buffer[axis];
            seq.buffer[axis] = 0.0;
            if buffered.abs() < f32::EPSILON {
                continue;
            }
            let amount = -buffered;
            let (dx, dy) = if axis == 0 {
                (amount, 0.0)
            } else {
                (0.0, amount)
            };
            scroll_out.push(ScrollRequest {
                path: seq.path.clone(),
                dx,
                dy,
            });
        }
    }
}

fn tap_family(
    order: &HashMap<u32, Vec<u32>>,
    decls: &[Option<GestureDecl>],
    path: &[u32],
) -> Option<TapFamily> {
    for target in path {
        let Some(ids) = order.get(target) else {
            continue;
        };
        let mut tap = None;
        let mut double = None;
        for id in ids {
            let Some(decl) = decls.get(*id as usize).and_then(Option::as_ref) else {
                continue;
            };
            if !decl.config.enabled {
                continue;
            }
            match decl.kind {
                GestureKind::Tap if decl.any_handler(&[1, 2, 3, 4]) => tap = Some(*id),
                GestureKind::DoubleTap if decl.has_handler(SLOT_DOUBLE_TAP) => double = Some(*id),
                _ => {}
            }
        }
        if tap.is_some() || double.is_some() {
            return Some(TapFamily {
                target: *target,
                tap,
                double,
            });
        }
    }
    None
}

fn emit_tap(events: &mut Vec<GestureEvent>, pending: &PendingTap) {
    if pending.tap_handler == 0 {
        return;
    }
    let mut event = GestureEvent::new(pending.time_ms);
    event.slot = SLOT_TAP;
    event.gesture = pending.gesture;
    event.sequence = pending.sequence;
    event.handler = pending.tap_handler;
    event.target = pending.target;
    event.x = pending.x;
    event.y = pending.y;
    events.push(event);
}

#[allow(clippy::too_many_arguments)]
fn emit_for(
    events: &mut Vec<GestureEvent>,
    decls: &[Option<GestureDecl>],
    id: u32,
    slot: u8,
    sequence: u32,
    time_ms: f64,
    x: f32,
    y: f32,
    reason: u8,
    target: u32,
) {
    push_event(events, decls, id, slot, sequence, time_ms, |event| {
        event.target = target;
        event.x = x;
        event.y = y;
        event.reason = reason;
    });
}

fn push_event(
    events: &mut Vec<GestureEvent>,
    decls: &[Option<GestureDecl>],
    id: u32,
    slot: u8,
    sequence: u32,
    time_ms: f64,
    fill: impl FnOnce(&mut GestureEvent),
) {
    let Some(Some(decl)) = decls.get(id as usize) else {
        return;
    };
    let Some(handler) = decl.events.get(slot as usize).copied() else {
        return;
    };
    if handler == 0 {
        return;
    }
    let mut event = GestureEvent::new(time_ms);
    event.slot = slot;
    event.gesture = id;
    event.sequence = sequence;
    event.handler = handler;
    event.target = decl.target;
    fill(&mut event);
    events.push(event);
}

fn wrap_angle(angle: f64) -> f64 {
    (angle + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
}

fn wrap_offset(last: f64, target: f64) -> f64 {
    let mut delta = target - last;
    while delta > std::f64::consts::PI {
        delta -= std::f64::consts::TAU;
    }
    while delta < -std::f64::consts::PI {
        delta += std::f64::consts::TAU;
    }
    delta
}

fn direction_matches(mask: u8, axis: u8, dx: f32, dy: f32) -> bool {
    let main = match axis {
        0 => dx,
        1 => dy,
        _ => {
            if dx.abs() >= dy.abs() {
                dx
            } else {
                dy
            }
        }
    };
    let (positive, negative) = match axis {
        0 => (2, 1),
        1 => (8, 4),
        _ => {
            if dx.abs() >= dy.abs() {
                (2, 1)
            } else {
                (8, 4)
            }
        }
    };
    if main >= 0.0 {
        if main == 0.0 && mask == 0 {
            return true;
        }
        mask & positive != 0 || mask & negative != 0
    } else {
        mask & negative != 0 || mask & positive != 0
    }
}

fn non_negative(value: &Value) -> Result<f32, String> {
    let number = value.as_f32()?;
    if number < 0.0 {
        return Err("gesture distance must not be negative".into());
    }
    Ok(number)
}

fn axis(value: &Value) -> Result<u8, String> {
    let Value::String(text) = value else {
        return Err("axis must be a string".into());
    };
    match text.as_str() {
        "x" => Ok(0),
        "y" => Ok(1),
        "both" => Ok(2),
        other => Err(format!("unsupported axis {other}")),
    }
}

fn direction(value: &Value) -> Result<u8, String> {
    let Value::String(text) = value else {
        return Err("direction must be a string".into());
    };
    let mut mask = 0u8;
    for part in text.split(',') {
        mask |= match part.trim() {
            "left" => 1,
            "right" => 2,
            "up" => 4,
            "down" => 8,
            "horizontal" => 3,
            "vertical" => 12,
            "both" | "any" | "" => 0,
            other => return Err(format!("unsupported direction {other}")),
        };
    }
    Ok(mask)
}

fn scroll_bits(value: &Value) -> Result<u8, String> {
    let Value::String(text) = value else {
        return Err("reserveScroll must be a string".into());
    };
    match text.as_str() {
        "none" => Ok(0),
        "x" => Ok(1),
        "y" => Ok(2),
        "both" => Ok(3),
        other => Err(format!("unsupported reserveScroll {other}")),
    }
}

fn set_component(config: &mut GestureConfig, bit: u8, on: bool) {
    if on {
        config.components |= bit;
    } else {
        config.components &= !bit;
    }
}

/// `BSGE` version 1: magic, version, count, then fixed records.
pub fn encode_events(events: &[GestureEvent]) -> Vec<u8> {
    let mut out = Vec::with_capacity(10 + events.len() * 66);
    out.extend_from_slice(b"BSGE");
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(events.len() as u32).to_le_bytes());
    for event in events {
        out.push(event.slot);
        out.extend_from_slice(&event.gesture.to_le_bytes());
        out.extend_from_slice(&event.sequence.to_le_bytes());
        out.extend_from_slice(&event.handler.to_le_bytes());
        out.extend_from_slice(&event.target.to_le_bytes());
        out.push(event.reason);
        out.extend_from_slice(&event.time_ms.to_le_bytes());
        for value in [
            event.x,
            event.y,
            event.translation_x,
            event.translation_y,
            event.delta_x,
            event.delta_y,
            event.scale,
            event.rotation,
            event.velocity_x,
            event.velocity_y,
        ] {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl(rec: &mut Recognizer, id: u32, target: u32, kind: GestureKind, handler: u32) {
        rec.spawn(id, target, kind, 0).unwrap();
        let slot = match kind {
            GestureKind::Tap => SLOT_TAP,
            GestureKind::DoubleTap => SLOT_DOUBLE_TAP,
            GestureKind::LongPress | GestureKind::Pan | GestureKind::Transform => SLOT_START,
        };
        rec.bind(id, slot, handler).unwrap();
    }

    fn down(x: f32, y: f32, path: &[u32]) -> PointerInput {
        PointerInput {
            action: PointerAction::Down,
            id: 1,
            x,
            y,
            time_ms: 0.0,
            buttons: 1,
            pointer_kind: 0,
            path: path.to_vec(),
        }
    }

    fn mv(x: f32, y: f32, time_ms: f64) -> PointerInput {
        PointerInput {
            action: PointerAction::Move,
            id: 1,
            x,
            y,
            time_ms,
            buttons: 1,
            pointer_kind: 0,
            path: Vec::new(),
        }
    }

    fn up(x: f32, y: f32, time_ms: f64) -> PointerInput {
        PointerInput {
            action: PointerAction::Up,
            id: 1,
            x,
            y,
            time_ms,
            buttons: 0,
            pointer_kind: 0,
            path: Vec::new(),
        }
    }

    #[test]
    fn tap_waits_for_the_up_sample() {
        let mut rec = Recognizer::new();
        decl(&mut rec, 2, 10, GestureKind::Tap, 7);
        rec.pointer(down(5.0, 5.0, &[10, 1]));
        assert!(rec.drain_events().is_empty());
        rec.pointer(up(6.0, 5.0, 10.0));
        let events = rec.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].slot, SLOT_TAP);
        assert_eq!(events[0].handler, 7);
    }

    #[test]
    fn drag_fails_tap_and_scrolls() {
        let mut rec = Recognizer::new();
        decl(&mut rec, 2, 10, GestureKind::Tap, 7);
        rec.pointer(down(5.0, 5.0, &[10, 1]));
        rec.pointer(mv(30.0, 5.0, 10.0));
        let scroll = rec.drain_scroll();
        assert!(!scroll.is_empty());
        assert_eq!(scroll[0].dx, -25.0);
        rec.pointer(up(30.0, 5.0, 20.0));
        assert!(rec.drain_events().iter().all(|event| event.slot != SLOT_TAP));
    }

    #[test]
    fn pan_wins_over_scrolling_on_its_axis() {
        let mut rec = Recognizer::new();
        decl(&mut rec, 2, 10, GestureKind::Pan, 7);
        rec.set_prop(2, "axis", &Value::String("x".into())).unwrap();
        rec.pointer(down(0.0, 0.0, &[10, 1]));
        rec.pointer(mv(15.0, 0.0, 10.0));
        let events = rec.drain_events();
        assert!(events.iter().any(|event| event.slot == SLOT_START));
        assert!(rec.drain_scroll().is_empty());
    }

    #[test]
    fn reserved_scroll_is_replayed_once_after_failure() {
        let mut rec = Recognizer::new();
        rec.spawn(2, 10, GestureKind::Pan, 0).unwrap();
        rec.bind(2, SLOT_START, 4).unwrap();
        rec.set_prop(2, "axis", &Value::String("x".into())).unwrap();
        rec.set_prop(2, "minDistance", &Value::Int(100)).unwrap();
        rec.set_prop(2, "reserveScroll", &Value::String("x".into()))
            .unwrap();
        rec.set_prop(2, "failCrossDistance", &Value::Int(1)).unwrap();
        rec.pointer(down(0.0, 0.0, &[10, 1]));
        rec.pointer(mv(10.0, 0.0, 10.0));
        assert!(rec.drain_scroll().is_empty());
        rec.pointer(mv(10.0, 20.0, 20.0));
        let scroll = rec.drain_scroll();
        let dx: f32 = scroll.iter().map(|request| request.dx).sum();
        let dy: f32 = scroll.iter().map(|request| request.dy).sum();
        assert_eq!(dx, -10.0);
        assert_eq!(dy, -20.0);
    }

    #[test]
    fn long_press_wins_and_blocks_the_tap() {
        let mut rec = Recognizer::new();
        decl(&mut rec, 2, 10, GestureKind::LongPress, 9);
        decl(&mut rec, 3, 10, GestureKind::Tap, 7);
        rec.pointer(down(5.0, 5.0, &[10, 1]));
        rec.advance(600.0);
        let events = rec.drain_events();
        assert!(events.iter().any(|event| event.slot == SLOT_START));
        rec.pointer(up(5.0, 5.0, 700.0));
        assert!(rec.drain_events().iter().all(|event| event.slot != SLOT_TAP));
    }

    #[test]
    fn double_tap_delays_and_then_fires() {
        let mut rec = Recognizer::new();
        decl(&mut rec, 2, 10, GestureKind::DoubleTap, 7);
        rec.pointer(down(5.0, 5.0, &[10, 1]));
        rec.pointer(up(5.0, 5.0, 10.0));
        assert!(rec.drain_events().is_empty());
        let mut second = down(6.0, 5.0, &[10, 1]);
        second.time_ms = 100.0;
        rec.pointer(second);
        rec.pointer(up(6.0, 5.0, 110.0));
        let events = rec.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].slot, SLOT_DOUBLE_TAP);
    }

    #[test]
    fn transform_upgrades_and_cancels_the_pan() {
        let mut rec = Recognizer::new();
        decl(&mut rec, 2, 10, GestureKind::Pan, 7);
        rec.bind(2, SLOT_CANCEL, 9).unwrap();
        decl(&mut rec, 3, 10, GestureKind::Transform, 8);
        rec.pointer(down(0.0, 0.0, &[10, 1]));
        rec.pointer(mv(20.0, 0.0, 10.0));
        let mut second = down(20.0, 0.0, &[10, 1]);
        second.id = 2;
        second.time_ms = 20.0;
        rec.pointer(second);
        let mut move2 = mv(60.0, 0.0, 30.0);
        move2.id = 2;
        rec.pointer(move2);
        let events = rec.drain_events();
        assert!(events.iter().any(|event| event.slot == SLOT_CANCEL
            && event.gesture == 2
            && event.reason == REASON_UPGRADE));
        assert!(events
            .iter()
            .any(|event| event.slot == SLOT_START && event.gesture == 3));
    }

    #[test]
    fn encoded_event_batch_roundtrips_its_header() {
        let event = GestureEvent {
            slot: SLOT_TAP,
            gesture: 3,
            sequence: 1,
            handler: 9,
            target: 4,
            ..GestureEvent::new(5.0)
        };
        let bytes = encode_events(&[event]);
        assert_eq!(&bytes[..4], b"BSGE");
        assert_eq!(u32::from_le_bytes(bytes[6..10].try_into().unwrap()), 1);
    }
}
