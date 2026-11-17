//! UI-thread shared variables and style declarations.
//!
//! A variable is a live UI-thread value: either driven by a state source
//! (pressed / focused / disabled / editing) or written by the script thread.
//! Declarations are virtual style components (`ViewStyle` / `TextStyle`): they
//! own no box, never hit, never paint, and stay out of Taffy and the semantic
//! tree. Each carries a `when` predicate plus one patch per paint property,
//! both compiled to expression programs (see [`crate::expr`]).
//!
//! Evaluation is declaration-cached: the shell re-evaluates only the
//! declarations a changed variable depends on, and paint merges the cached
//! patches without running any expression. Declarations are ordered per target
//! and a later matching declaration wins.

use std::collections::HashSet;

use crate::color::Color;
use crate::expr::{ExprValue, Program};
use crate::ordered::OrderedDecls;
use crate::props::{TEXT_STYLE_PROPS, VIEW_STYLE_PROPS};

/// State source kinds. `0` is "no source": the script thread writes the value.
pub const SOURCE_NONE: u8 = 0;
pub const SOURCE_PRESSED: u8 = 1;
pub const SOURCE_FOCUSED: u8 = 2;
pub const SOURCE_DISABLED: u8 = 3;
pub const SOURCE_EDITING: u8 = 4;
/// Source read: pan translation along x of one gesture declaration, logical px.
pub const SOURCE_GESTURE_TX: u8 = 5;
/// Source read: pan translation along y of one gesture declaration, logical px.
pub const SOURCE_GESTURE_TY: u8 = 6;
/// Source read: pinch scale of one transform gesture declaration; `1.0` idle.
pub const SOURCE_GESTURE_SCALE: u8 = 7;
/// Source read: pinch rotation of one transform gesture declaration, radians
/// around the gesture center.
pub const SOURCE_GESTURE_ROTATION: u8 = 8;
/// Source read: horizontal scroll offset of one scroll container, logical px.
pub const SOURCE_SCROLL_X: u8 = 9;
/// Source read: vertical scroll offset of one scroll container, logical px.
pub const SOURCE_SCROLL_Y: u8 = 10;

/// Interest bits: which state sources a declaration's programs may read,
/// precomputed from the variables the programs load. The hit path consults
/// only these bits, never runs expressions.
pub(crate) const INT_PRESSED: u8 = 1;
pub(crate) const INT_FOCUSED: u8 = 2;
pub(crate) const INT_DISABLED: u8 = 4;
pub(crate) const INT_EDITING: u8 = 8;

/// Paint patch slots. Slot layout is shared by both components; the compiler
/// and the runtime enforce which slots each component may set.
pub(crate) const SLOT_BACKGROUND: usize = 0;
pub(crate) const SLOT_COLOR: usize = 1;
pub(crate) const SLOT_OPACITY: usize = 2;
pub(crate) const SLOT_RADIUS: usize = 3;
pub(crate) const SLOT_TEXT_ALIGN: usize = 4;
pub(crate) const SLOT_FIT: usize = 5;
pub(crate) const SLOT_TRANSLATE_X: usize = 6;
pub(crate) const SLOT_TRANSLATE_Y: usize = 7;
pub(crate) const SLOT_SCALE: usize = 8;
pub(crate) const SLOT_ROTATE: usize = 9;
pub(crate) const SLOT_COUNT: usize = 10;

/// Easing codes for tweens.
pub const EASE_LINEAR: u8 = 0;
pub const EASE_IN_OUT: u8 = 1;

/// How one write transitions. The animation belongs to the write that
/// started it: styles that read the variable get the motion for free.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimSpec {
    Tween { duration_ms: f32, easing: u8 },
    Spring { stiffness: f32, damping: f32 },
}

/// Easing in `[0, 1] -> [0, 1]`. `easeInOut` is easeInOutQuad: the second
/// half eases out of the remaining distance (`1 - 2*(1-t)^2`). Using
/// `(2t-1)` there peaks at 1 halfway through and runs to -1 at the end.
pub fn ease(easing: u8, t: f32) -> f32 {
    match easing {
        EASE_IN_OUT => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                let u = 1.0 - t;
                1.0 - 2.0 * u * u
            }
        }
        _ => t,
    }
}

/// Interpolate two values of the same kind. Bool animates instantly; a Bool
/// spec therefore never starts and the write converges at once.
pub fn mix_values(from: &ExprValue, to: &ExprValue, t: f32) -> Option<ExprValue> {
    Some(match (from, to) {
        (ExprValue::Num(a), ExprValue::Num(b)) => ExprValue::Num(a + (b - a) * t),
        (ExprValue::Color(a), ExprValue::Color(b)) => ExprValue::Color(a.mix(b, t)),
        _ => return None,
    })
}

/// Slot name to index. Both components share the same property names.
pub(crate) fn slot_of(key: &str) -> Option<usize> {
    Some(match key {
        "backgroundColor" => SLOT_BACKGROUND,
        "color" => SLOT_COLOR,
        "opacity" => SLOT_OPACITY,
        "radius" => SLOT_RADIUS,
        "textAlign" => SLOT_TEXT_ALIGN,
        "fit" => SLOT_FIT,
        "translateX" => SLOT_TRANSLATE_X,
        "translateY" => SLOT_TRANSLATE_Y,
        "scale" => SLOT_SCALE,
        "rotate" => SLOT_ROTATE,
        _ => return None,
    })
}

/// Whether `component` may patch `slot`. Components differ only in this
/// compile-time whitelist.
pub(crate) fn slot_allowed(component: u8, slot: usize) -> bool {
    let props = if component == 1 {
        VIEW_STYLE_PROPS
    } else {
        TEXT_STYLE_PROPS
    };
    let name = match slot {
        SLOT_BACKGROUND => "backgroundColor",
        SLOT_COLOR => "color",
        SLOT_OPACITY => "opacity",
        SLOT_RADIUS => "radius",
        SLOT_TEXT_ALIGN => "textAlign",
        SLOT_FIT => "fit",
        SLOT_TRANSLATE_X => "translateX",
        SLOT_TRANSLATE_Y => "translateY",
        SLOT_SCALE => "scale",
        SLOT_ROTATE => "rotate",
        _ => return false,
    };
    props.contains(&name)
}

/// Slot type the patch value must produce. `textAlign` and `fit` resolve to
/// their numeric enum codes, validated per evaluation.
pub(crate) fn slot_type(slot: usize) -> SlotType {
    match slot {
        SLOT_BACKGROUND | SLOT_COLOR => SlotType::Color,
        SLOT_TEXT_ALIGN | SLOT_FIT => SlotType::Enum,
        _ => SlotType::Num,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SlotType {
    Num,
    Color,
    Enum,
}

/// One source-linked or script-written variable.
#[derive(Clone, Debug)]
pub(crate) struct Var {
    pub value: ExprValue,
    pub source: u8,
    /// Node handle the source samples. `0` until a `BindVar` op attaches the
    /// target (refs resolve after the node spawns) or for script-driven vars.
    pub target: u32,
    /// Declarations that read this variable.
    pub dependents: Vec<u32>,
    /// In-flight transition, if the last write started one.
    pub anim_state: Option<AnimState>,
}

/// One in-flight transition. Both tweens and springs drive a progress scalar
/// from 0 toward 1 and interpolate `from -> to` with it, so Num and Color
/// share the same integrator; spring overshoot extrapolates through `mix`.
#[derive(Clone, Debug)]
pub struct AnimState {
    pub spec: AnimSpec,
    pub from: ExprValue,
    pub to: ExprValue,
    pub start_ms: f64,
    pub progress: f32,
    /// Spring velocity in progress units per second.
    pub velocity: f32,
    pub last_ms: f64,
}

/// A progress close enough to 1 (and a small enough velocity) to snap a
/// spring; bounded so the caller can also clamp far overshoots.
const SPRING_EPS: f32 = 0.0005;
const SPRING_VELOCITY_EPS: f32 = 0.002;
/// Sub-step width for spring integration, in ms.
const SPRING_STEP_MS: f64 = 8.0;

impl AnimState {
    pub fn new(spec: AnimSpec, from: ExprValue, to: ExprValue, now_ms: f64) -> Self {
        Self {
            spec,
            from,
            to,
            start_ms: now_ms,
            progress: 0.0,
            velocity: 0.0,
            last_ms: now_ms,
        }
    }

    /// Advance to `now_ms`. Returns the visible value when it changed plus
    /// whether the transition converged on `to`.
    pub fn step(&mut self, now_ms: f64) -> (Option<ExprValue>, bool) {
        match self.spec {
            AnimSpec::Tween {
                duration_ms,
                easing,
            } => {
                let duration = (duration_ms as f64).max(1.0);
                let t = ((now_ms - self.start_ms) / duration).clamp(0.0, 1.0) as f32;
                self.progress = ease(easing, t);
                let converged = t >= 1.0;
                if converged {
                    self.progress = 1.0;
                }
                (
                    Some(
                        mix_values(&self.from, &self.to, self.progress)
                            .unwrap_or_else(|| self.to.clone()),
                    ),
                    converged,
                )
            }
            AnimSpec::Spring { stiffness, damping } => {
                // Sub-step so a coarse or long tick stays stable.
                let mut remaining = (now_ms - self.last_ms).max(0.0);
                self.last_ms = now_ms;
                let mut x = self.progress;
                let mut v = self.velocity;
                while remaining > 0.0 {
                    let dt = (remaining.min(SPRING_STEP_MS) / 1000.0) as f32;
                    remaining -= SPRING_STEP_MS;
                    let acceleration = -stiffness * (x - 1.0) - damping * v;
                    v += acceleration * dt;
                    x += v * dt;
                }
                self.velocity = v;
                self.progress = x;
                let converged = (1.0 - x).abs() < SPRING_EPS && v.abs() < SPRING_VELOCITY_EPS;
                if converged {
                    self.progress = 1.0;
                    self.velocity = 0.0;
                }
                (
                    Some(
                        mix_values(&self.from, &self.to, self.progress)
                            .unwrap_or_else(|| self.to.clone()),
                    ),
                    converged,
                )
            }
        }
    }
}

impl Var {
    pub fn value_type(&self) -> crate::expr::Type {
        match &self.value {
            ExprValue::Num(_) => crate::expr::Type::N,
            ExprValue::Color(_) => crate::expr::Type::C,
            ExprValue::Bool(_) => crate::expr::Type::B,
        }
    }
}

#[derive(Default)]
pub(crate) struct VarTable {
    vars: Vec<Option<Var>>,
    retired: HashSet<u32>,
}

impl VarTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, id: u32) -> Option<&Var> {
        self.vars.get(id as usize).and_then(Option::as_ref)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Var> {
        self.vars.get_mut(id as usize).and_then(Option::as_mut)
    }

    /// Claim `id`. Variables are never reused; a retired id stays retired.
    pub fn create(
        &mut self,
        id: u32,
        source: u8,
        target: u32,
        value: ExprValue,
    ) -> Result<(), String> {
        if id == 0 || self.retired.contains(&id) || self.get(id).is_some() {
            return Err(format!("var {id} is not free"));
        }
        if self.vars.len() <= id as usize {
            self.vars.resize_with(id as usize + 1, || None);
        }
        self.vars[id as usize] = Some(Var {
            value,
            source,
            target,
            dependents: Vec::new(),
            anim_state: None,
        });
        Ok(())
    }

    /// Retire `id` and return the declarations that read it, so callers can
    /// mark them as not matching.
    pub fn dispose(&mut self, id: u32) -> Result<Vec<u32>, String> {
        let Some(var) = self.vars.get(id as usize).and_then(Option::as_ref) else {
            return Err(format!("unknown var {id}"));
        };
        let dependents = var.dependents.clone();
        if let Some(slot) = self.vars.get_mut(id as usize) {
            *slot = None;
        }
        self.retired.insert(id);
        Ok(dependents)
    }

    /// `(id, source, target)` for every source-driven variable, in id order.
    pub fn all_with_source(&self) -> Vec<(u32, u8, u32)> {
        self.vars
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                let var = slot.as_ref()?;
                (var.source != SOURCE_NONE).then(|| (index as u32, var.source, var.target))
            })
            .collect()
    }

    /// Every live variable id, for the animation tick.
    pub fn all_with_anim(&self) -> Vec<u32> {
        self.vars
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.as_ref().map(|_| index as u32))
            .collect()
    }

    /// Whether any variable still carries an in-flight transition.
    pub fn any_animating(&self) -> bool {
        self.vars
            .iter()
            .any(|slot| slot.as_ref().is_some_and(|var| var.anim_state.is_some()))
    }

    /// Variables sourced at a destroyed node; callers dispose them.
    pub fn retire_target(&mut self, target: u32) -> Vec<u32> {
        self.all_with_source()
            .into_iter()
            .filter(|(_, _, var_target)| *var_target == target)
            .map(|(id, _, _)| id)
            .collect()
    }
}

/// One compiled slot value: a folded constant or a program.
#[derive(Clone, Debug)]
pub(crate) enum DeclExpr {
    Static(ExprValue),
    Program(Box<Program>),
}

/// Resolved paint overrides keyed by slot. `None` leaves the node's base
/// value visible.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ResolvedPatch {
    pub background: Option<Color>,
    pub color: Option<Color>,
    pub opacity: Option<f32>,
    pub radius: Option<f32>,
    pub text_align: Option<u8>,
    pub fit: Option<u8>,
    /// Paint-side transform components. Translate is in logical px, scale is
    /// a multiplier, rotate is radians around the node center.
    pub translate_x: Option<f32>,
    pub translate_y: Option<f32>,
    pub scale: Option<f32>,
    pub rotate: Option<f32>,
}

impl ResolvedPatch {
    fn apply(&mut self, other: &Self) {
        if other.background.is_some() {
            self.background = other.background;
        }
        if other.color.is_some() {
            self.color = other.color;
        }
        if other.opacity.is_some() {
            self.opacity = other.opacity;
        }
        if other.radius.is_some() {
            self.radius = other.radius;
        }
        if other.text_align.is_some() {
            self.text_align = other.text_align;
        }
        if other.fit.is_some() {
            self.fit = other.fit;
        }
        if other.translate_x.is_some() {
            self.translate_x = other.translate_x;
        }
        if other.translate_y.is_some() {
            self.translate_y = other.translate_y;
        }
        if other.scale.is_some() {
            self.scale = other.scale;
        }
        if other.rotate.is_some() {
            self.rotate = other.rotate;
        }
    }
}

/// One virtual style declaration.
#[derive(Clone)]
pub(crate) struct StyleDecl {
    /// `1` ViewStyle, `2` TextStyle. Enforces the compile-time whitelist.
    pub component: u8,
    pub target: u32,
    /// `None` means `when` was omitted, which always matches.
    pub when: Option<DeclExpr>,
    pub patch: [Option<DeclExpr>; SLOT_COUNT],
    pub interests: u8,
    pub resolved_when: bool,
    pub resolved: ResolvedPatch,
}

/// Declarations plus per-target order. Ids are independent of node handles.
pub(crate) struct StyleDecls {
    ordered: OrderedDecls<StyleDecl>,
    /// var id -> decl ids that read it, rebuilt by declaration registration.
    deps: std::collections::HashMap<u32, Vec<u32>>,
}

impl StyleDecls {
    pub fn new() -> Self {
        Self {
            ordered: OrderedDecls::new(),
            deps: std::collections::HashMap::new(),
        }
    }

    pub fn decl(&self, id: u32) -> Option<&StyleDecl> {
        self.ordered.decl(id)
    }

    pub fn decl_mut(&mut self, id: u32) -> Option<&mut StyleDecl> {
        self.ordered.decl_mut(id)
    }

    pub fn spawn(&mut self, id: u32, target: u32, component: u8, index: u32) -> Result<(), String> {
        self.ordered.insert(
            id,
            target,
            index,
            "style declaration",
            StyleDecl {
                component,
                target,
                when: None,
                patch: Default::default(),
                interests: 0,
                resolved_when: false,
                resolved: ResolvedPatch::default(),
            },
        )
    }

    /// Replace `when` or one patch slot. The value is already decoded into a
    /// [`DeclExpr`]; interest recomputation is the caller's job.
    pub fn set_slot(
        &mut self,
        id: u32,
        slot: Option<usize>,
        value: Option<DeclExpr>,
    ) -> Result<(), String> {
        let decl = self
            .decl_mut(id)
            .ok_or_else(|| format!("unknown style declaration {id}"))?;
        match slot {
            None => decl.when = value,
            Some(slot) => decl.patch[slot] = value,
        }
        Ok(())
    }

    /// `index` is the final position after the declaration leaves its old slot.
    pub fn move_decl(&mut self, id: u32, target: u32, index: u32) -> Result<(), String> {
        let old_target = self
            .decl(id)
            .ok_or_else(|| format!("unknown style declaration {id}"))?
            .target;
        self.ordered
            .move_decl(id, old_target, target, index, "style declaration")?;
        if let Some(decl) = self.decl_mut(id) {
            decl.target = target;
        }
        Ok(())
    }

    pub fn dispose(&mut self, id: u32) -> Result<(), String> {
        let target = self
            .decl(id)
            .ok_or_else(|| format!("unknown style declaration {id}"))?
            .target;
        self.unregister_deps(id);
        self.ordered.remove(id, target);
        Ok(())
    }

    /// Drop every declaration attached to a destroyed node and return their ids.
    pub fn target_disposed(&mut self, target: u32) -> Vec<u32> {
        let ids = self.ordered.retire_target(target);
        for id in &ids {
            self.unregister_deps(*id);
        }
        ids
    }

    /// Recompute the interest bits from the variables the programs load.
    pub fn register_deps(&mut self, id: u32, vars: &VarTable) {
        self.unregister_deps(id);
        let Some(decl) = self.decl(id) else {
            return;
        };
        let mut interests = 0;
        let mut used: Vec<u32> = Vec::new();
        let mut collect = |expr: &Option<DeclExpr>| {
            if let Some(DeclExpr::Program(program)) = expr {
                for var in program.used_vars() {
                    if !used.contains(&var) {
                        used.push(var);
                    }
                }
            }
        };
        collect(&decl.when);
        for slot in &decl.patch {
            collect(slot);
        }
        for var in used {
            if let Some(var) = vars.get(var) {
                interests |= match var.source {
                    SOURCE_PRESSED => INT_PRESSED,
                    SOURCE_FOCUSED => INT_FOCUSED,
                    SOURCE_DISABLED => INT_DISABLED,
                    SOURCE_EDITING => INT_EDITING,
                    _ => 0,
                };
            }
            self.deps.entry(var).or_default().push(id);
        }
        if let Some(decl) = self.decl_mut(id) {
            decl.interests = interests;
        }
    }

    fn unregister_deps(&mut self, id: u32) {
        for list in self.deps.values_mut() {
            list.retain(|existing| *existing != id);
        }
        self.deps.retain(|_, list| !list.is_empty());
    }

    /// Declarations that read any of `vars`, deduplicated.
    pub fn dependents_of(&self, vars: impl IntoIterator<Item = u32>) -> Vec<u32> {
        let mut out = Vec::new();
        for var in vars {
            if let Some(list) = self.deps.get(&var) {
                for id in list {
                    if !out.contains(id) {
                        out.push(*id);
                    }
                }
            }
        }
        out
    }

    /// Whether `target` still wants pressed feedback: any declaration that may
    /// read the pressed source, whether or not `when` currently holds.
    pub fn pressed_interest(&self, target: u32) -> bool {
        let Some(ids) = self.ordered.ids(target) else {
            return false;
        };
        ids.iter().any(|id| {
            self.decl(*id)
                .is_some_and(|decl| decl.interests & INT_PRESSED != 0)
        })
    }

    /// Whether a pressed-interest declaration currently matches; the pressed
    /// state drops when no matching declaration remains.
    pub fn pressed_feedback(&self, target: u32) -> bool {
        let Some(ids) = self.ordered.ids(target) else {
            return false;
        };
        ids.iter().any(|id| {
            self.decl(*id)
                .is_some_and(|decl| decl.interests & INT_PRESSED != 0 && decl.resolved_when)
        })
    }

    /// Whether a focused-interest declaration currently matches; such a
    /// declaration replaces the default focus ring, even when it patches
    /// nothing.
    pub fn focus_style_taken(&self, target: u32) -> bool {
        let Some(ids) = self.ordered.ids(target) else {
            return false;
        };
        ids.iter().any(|id| {
            self.decl(*id)
                .is_some_and(|decl| decl.interests & INT_FOCUSED != 0 && decl.resolved_when)
        })
    }

    /// Merge the currently-matching declarations on `target`, in order.
    pub fn overlay(&self, target: u32, out: &mut ResolvedPatch) {
        let Some(ids) = self.ordered.ids(target) else {
            return;
        };
        for id in ids {
            let Some(decl) = self.decl(*id) else {
                continue;
            };
            if decl.resolved_when {
                out.apply(&decl.resolved);
            }
        }
    }
}

/// A script-driven variable finished converging on the UI thread.
///
/// Stage 2 has no animation, so convergence is immediate and the shell emits
/// the event as the batch applies; the animation tick will delay it later.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VarEvent {
    pub var: u32,
}

/// `BSVE` version 1. Little-endian batch of settled variable ids.
pub fn encode_var_events(events: &[VarEvent]) -> Vec<u8> {
    let mut out = Vec::with_capacity(10 + events.len() * 4);
    out.extend_from_slice(b"BSVE");
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(events.len() as u32).to_le_bytes());
    for event in events {
        out.extend_from_slice(&event.var.to_le_bytes());
    }
    out
}
