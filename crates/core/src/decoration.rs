//! Virtual paint decorations.
//!
//! `Border`, `Outline`, and `Shadow` attach to a real node by handle, keep a
//! per-target order, and never enter taffy, hit testing, or the semantic tree.
//! Expressions are cached on the declaration; paint reads only that cache.

use crate::color::Color;
use crate::expr::{ExprValue, Program, Type};
use crate::ordered::OrderedDecls;
use crate::state_style::{
    AnimSpec, AnimState, DeclExpr, TransitionSpec, VarTable, INT_DISABLED, INT_EDITING,
    INT_FOCUSED, INT_PRESSED, SOURCE_DISABLED, SOURCE_EDITING, SOURCE_FOCUSED, SOURCE_PRESSED,
};

pub const KIND_BORDER: u8 = 1;
pub const KIND_OUTLINE: u8 = 2;
pub const KIND_SHADOW: u8 = 3;

/// Paint phase. Background is behind the host fill, underlay is between the
/// fill and content, overlay is after the host's own clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DecorationLayer {
    Background = 0,
    Underlay = 1,
    Overlay = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BorderAlign {
    Inside = 0,
    Center = 1,
    Outside = 2,
}

/// Axis-aligned rounded rectangle in logical px. `radius` is already clamped
/// to half the shorter side when the rect came from [`host_frame`] or [`expand`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundedRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub radius: f32,
}

/// Host frame used as the decoration basis. Non-positive size draws nothing.
pub(crate) fn host_frame(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<RoundedRect> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    Some(RoundedRect {
        x,
        y,
        w,
        h,
        radius: clamp_radius(radius, w, h),
    })
}

/// Outset `frame` by `d` on every side. `d` may be negative. A non-positive
/// result is empty (`None`); radius is clamped only after the new size is known.
pub(crate) fn expand(frame: RoundedRect, d: f32) -> Option<RoundedRect> {
    let w = frame.w + 2.0 * d;
    let h = frame.h + 2.0 * d;
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    Some(RoundedRect {
        x: frame.x - d,
        y: frame.y - d,
        w,
        h,
        radius: clamp_radius(frame.radius + d, w, h),
    })
}

fn clamp_radius(radius: f32, w: f32, h: f32) -> f32 {
    if !radius.is_finite() || radius <= 0.0 || w <= 0.0 || h <= 0.0 {
        return 0.0;
    }
    radius.min(w.min(h) * 0.5)
}

/// Replace the frame's basis radius. `None` keeps the host effective radius.
pub(crate) fn with_radius(frame: RoundedRect, radius: Option<f32>) -> RoundedRect {
    let radius = radius.unwrap_or(frame.radius);
    RoundedRect {
        radius: clamp_radius(radius, frame.w, frame.h),
        ..frame
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ring {
    pub outer: RoundedRect,
    pub inner: Option<RoundedRect>,
}

/// Concentric ring. A collapsed inner contour fills the outer shape. A
/// non-positive outer contour draws nothing.
fn ring(base: RoundedRect, outer_d: f32, inner_d: f32) -> Option<Ring> {
    Some(Ring {
        outer: expand(base, outer_d)?,
        inner: expand(base, inner_d),
    })
}

pub(crate) fn border_ring(base: RoundedRect, align: BorderAlign, width: f32) -> Option<Ring> {
    let width = nonneg(width);
    if width == 0.0 {
        return None;
    }
    match align {
        BorderAlign::Inside => ring(base, 0.0, -width),
        BorderAlign::Center => ring(base, width * 0.5, -width * 0.5),
        BorderAlign::Outside => ring(base, width, 0.0),
    }
}

pub(crate) fn outline_ring(base: RoundedRect, offset: f32, width: f32) -> Option<Ring> {
    let width = nonneg(width);
    if width == 0.0 {
        return None;
    }
    ring(base, offset + width, offset)
}

/// Shadow caster after spread and translation. A collapsed spread draws nothing.
/// `knockout` is the host frame and is not modified here.
pub(crate) fn shadow_caster(base: RoundedRect, spread: f32, offset_x: f32, offset_y: f32) -> Option<RoundedRect> {
    let mut caster = expand(base, spread)?;
    caster.x += offset_x;
    caster.y += offset_y;
    Some(caster)
}

fn nonneg(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ResolvedDecoration {
    pub matched: bool,
    pub color: Color,
    /// `None` follows the host effective radius. Stored raw; paint clamps.
    pub radius: Option<f32>,
    pub width: f32,
    pub offset: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
}

impl Default for ResolvedDecoration {
    fn default() -> Self {
        Self {
            matched: false,
            color: Color::TRANSPARENT,
            radius: None,
            width: 1.0,
            offset: 0.0,
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 0.0,
        }
    }
}

#[derive(Clone)]
pub(crate) struct Decoration {
    pub target: u32,
    pub kind: u8,
    pub layer: DecorationLayer,
    pub align: BorderAlign,
    when: Option<DeclExpr>,
    color: Option<DeclExpr>,
    radius: Option<DeclExpr>,
    width: Option<DeclExpr>,
    offset: Option<DeclExpr>,
    offset_x: Option<DeclExpr>,
    offset_y: Option<DeclExpr>,
    blur: Option<DeclExpr>,
    spread: Option<DeclExpr>,
    pub interests: u8,
    used_vars: Vec<u32>,
    pub resolved: ResolvedDecoration,
    pub shown: ResolvedDecoration,
    motion: [Option<AnimState>; 8],
    awaiting: u8,
}
impl Decoration {
    fn fresh(target: u32, kind: u8) -> Self {
        Self {
            target,
            kind,
            layer: default_layer(kind),
            align: BorderAlign::Inside,
            when: None,
            color: None,
            radius: None,
            width: None,
            offset: None,
            offset_x: None,
            offset_y: None,
            blur: None,
            spread: None,
            interests: 0,
            used_vars: Vec::new(),
            resolved: ResolvedDecoration {
                matched: true,
                ..ResolvedDecoration::default()
            },
            shown: ResolvedDecoration {
                matched: true,
                ..ResolvedDecoration::default()
            },
            motion: [const { None }; 8],
            awaiting: 0,
        }
    }
}

fn default_layer(kind: u8) -> DecorationLayer {
    if kind == KIND_SHADOW {
        DecorationLayer::Background
    } else {
        DecorationLayer::Overlay
    }
}

pub(crate) struct DecorationDecls {
    ordered: OrderedDecls<Decoration>,
    deps: std::collections::HashMap<u32, Vec<u32>>,
}

impl DecorationDecls {
    pub(crate) fn new() -> Self {
        Self {
            ordered: OrderedDecls::new(),
            deps: std::collections::HashMap::new(),
        }
    }

    pub(crate) fn decl(&self, id: u32) -> Option<&Decoration> {
        self.ordered.decl(id)
    }

    pub(crate) fn ids(&self, target: u32) -> Option<&[u32]> {
        self.ordered.ids(target)
    }

    pub(crate) fn spawn(&mut self, id: u32, target: u32, kind: u8, index: u32) -> Result<(), String> {
        if !matches!(kind, KIND_BORDER | KIND_OUTLINE | KIND_SHADOW) {
            return Err(format!("unknown decoration kind {kind}"));
        }
        self.ordered
            .insert(id, target, index, "decoration", Decoration::fresh(target, kind))
    }

    pub(crate) fn move_decl(&mut self, id: u32, target: u32, index: u32) -> Result<(), String> {
        let old_target = self
            .decl(id)
            .ok_or_else(|| format!("unknown decoration {id}"))?
            .target;
        self.ordered
            .move_decl(id, old_target, target, index, "decoration")?;
        if let Some(decl) = self.ordered.decl_mut(id) {
            decl.target = target;
        }
        Ok(())
    }

    pub(crate) fn dispose(&mut self, id: u32) -> Result<(), String> {
        let target = self
            .decl(id)
            .ok_or_else(|| format!("unknown decoration {id}"))?
            .target;
        self.unregister_deps(id);
        self.ordered.remove(id, target);
        Ok(())
    }

    pub(crate) fn target_disposed(&mut self, target: u32) -> Vec<u32> {
        let ids = self.ordered.retire_target(target);
        for id in &ids {
            self.unregister_deps(*id);
        }
        ids
    }

    pub(crate) fn set_layer(&mut self, id: u32, layer: DecorationLayer) -> Result<(), String> {
        self.ordered
            .decl_mut(id)
            .ok_or_else(|| format!("unknown decoration {id}"))?
            .layer = layer;
        Ok(())
    }

    pub(crate) fn set_align(&mut self, id: u32, align: BorderAlign) -> Result<(), String> {
        let decl = self
            .ordered
            .decl_mut(id)
            .ok_or_else(|| format!("unknown decoration {id}"))?;
        if decl.kind != KIND_BORDER {
            return Err("this decoration has no property align".into());
        }
        decl.align = align;
        Ok(())
    }

    /// Store one programmable slot. `None` restores the slot default.
    pub(crate) fn set_slot(
        &mut self,
        id: u32,
        key: &str,
        value: Option<DeclExpr>,
    ) -> Result<(), String> {
        let decl = self
            .ordered
            .decl_mut(id)
            .ok_or_else(|| format!("unknown decoration {id}"))?;
        if !key_allowed(decl.kind, key) {
            return Err(format!("this decoration has no property {key}"));
        }
        match key {
            "when" => decl.when = value,
            "color" => decl.color = value,
            "radius" => decl.radius = value,
            "width" => decl.width = value,
            "offset" => decl.offset = value,
            "offsetX" => decl.offset_x = value,
            "offsetY" => decl.offset_y = value,
            "blur" => decl.blur = value,
            "spread" => decl.spread = value,
            other => return Err(format!("decoration has no property {other}")),
        }
        Ok(())
    }

    pub(crate) fn register_deps(&mut self, id: u32, vars: &VarTable) {
        self.unregister_deps(id);
        let Some(decl) = self.decl(id) else {
            return;
        };
        let mut interests = 0u8;
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
        collect(&decl.color);
        collect(&decl.radius);
        collect(&decl.width);
        collect(&decl.offset);
        collect(&decl.offset_x);
        collect(&decl.offset_y);
        collect(&decl.blur);
        collect(&decl.spread);
        for var in &used {
            if let Some(var) = vars.get(*var) {
                interests |= match var.source {
                    SOURCE_PRESSED => INT_PRESSED,
                    SOURCE_FOCUSED => INT_FOCUSED,
                    SOURCE_DISABLED => INT_DISABLED,
                    SOURCE_EDITING => INT_EDITING,
                    _ => 0,
                };
            }
            self.deps.entry(*var).or_default().push(id);
        }
        if let Some(decl) = self.ordered.decl_mut(id) {
            decl.interests = interests;
            decl.used_vars = used;
        }
    }

    fn unregister_deps(&mut self, id: u32) {
        for list in self.deps.values_mut() {
            list.retain(|existing| *existing != id);
        }
        self.deps.retain(|_, list| !list.is_empty());
    }

    pub(crate) fn dependents_of(&self, vars: impl IntoIterator<Item = u32>) -> Vec<u32> {
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

    pub(crate) fn pressed_interest(&self, target: u32) -> bool {
        let Some(ids) = self.ordered.ids(target) else {
            return false;
        };
        ids.iter().any(|id| {
            self.decl(*id)
                .is_some_and(|decl| decl.interests & INT_PRESSED != 0)
        })
    }

    pub(crate) fn pressed_feedback(&self, target: u32) -> bool {
        let Some(ids) = self.ordered.ids(target) else {
            return false;
        };
        ids.iter().any(|id| {
            self.decl(*id).is_some_and(|decl| {
                decl.interests & INT_PRESSED != 0 && decl.resolved.matched
            })
        })
    }
    pub(crate) fn mark_unmatched(&mut self, id: u32) {
        if let Some(decl) = self.ordered.decl_mut(id) {
            decl.resolved = ResolvedDecoration::default();
        }
    }


    /// Re-evaluate one decoration. A failed slot clears the cached effect.
    pub(crate) fn eval(&mut self, id: u32, vars: &VarTable) -> bool {
        let snapshot = {
            let Some(decl) = self.decl(id) else {
                return false;
            };
            (
                decl.resolved,
                decl.when.clone(),
                decl.color.clone(),
                decl.radius.clone(),
                decl.width.clone(),
                decl.offset.clone(),
                decl.offset_x.clone(),
                decl.offset_y.clone(),
                decl.blur.clone(),
                decl.spread.clone(),
            )
        };
        let next = resolve_slots(snapshot.1, snapshot.2, snapshot.3, snapshot.4, snapshot.5, snapshot.6, snapshot.7, snapshot.8, snapshot.9, vars);
        let changed = next != snapshot.0;
        if let Some(decl) = self.ordered.decl_mut(id) {
            decl.resolved = next;
        }
        changed
    }
    /// Move the painted cache toward `resolved`. `specs` follows color, radius,
    /// width, offset, offsetX, offsetY, blur, spread. Missing or zero-duration
    /// specs snap. An unmatched declaration drops its tweens.
    pub(crate) fn retarget(
        &mut self,
        id: u32,
        before: ResolvedDecoration,
        specs: [Option<TransitionSpec>; 8],
        now_ms: f64,
    ) {
        let Some(decl) = self.ordered.decl_mut(id) else {
            return;
        };
        let after = decl.resolved;
        if !after.matched {
            decl.shown = after;
            decl.motion = [const { None }; 8];
            decl.awaiting = 0;
            return;
        }
        chase_color(decl, 0, before.color, after.color, specs[0], now_ms);
        match (before.radius, after.radius) {
            (Some(from), Some(to)) => chase_num(decl, 1, from, to, specs[1], now_ms, true),
            _ if before.radius != after.radius => {
                decl.shown.radius = after.radius;
                decl.motion[1] = None;
                decl.awaiting &= !(1 << 1);
            }
            _ => {}
        }
        chase_num(decl, 2, before.width, after.width, specs[2], now_ms, false);
        chase_num(decl, 3, before.offset, after.offset, specs[3], now_ms, false);
        chase_num(decl, 4, before.offset_x, after.offset_x, specs[4], now_ms, false);
        chase_num(decl, 5, before.offset_y, after.offset_y, specs[5], now_ms, false);
        chase_num(decl, 6, before.blur, after.blur, specs[6], now_ms, false);
        chase_num(decl, 7, before.spread, after.spread, specs[7], now_ms, false);
        decl.shown.matched = true;
    }

    pub(crate) fn step_motion(&mut self, now_ms: f64) -> bool {
        let mut changed = false;
        for decl in self.ordered.decls.iter_mut().flatten() {
            for index in 0..8 {
                if decl.motion[index].is_none() {
                    continue;
                }
                let bit = 1u8 << index;
                if decl.awaiting & bit != 0 {
                    if let Some(anim) = decl.motion[index].as_mut() {
                        anim.start_ms = now_ms;
                        anim.last_ms = now_ms;
                    }
                    decl.awaiting &= !bit;
                }
                let (next, done, to) = {
                    let Some(anim) = decl.motion[index].as_mut() else {
                        continue;
                    };
                    let (next, done) = anim.step(now_ms);
                    (next, done, anim.to)
                };
                if let Some(next) = next {
                    write_shown(&mut decl.shown, index, next);
                    changed = true;
                }
                if done {
                    write_shown(&mut decl.shown, index, to);
                    decl.motion[index] = None;
                    changed = true;
                }
            }
        }
        changed
    }
}
fn chase_color(
    decl: &mut Decoration,
    index: usize,
    before: Color,
    after: Color,
    spec: Option<TransitionSpec>,
    now_ms: f64,
) {
    if before == after {
        return;
    }
    let bit = 1u8 << index;
    match spec {
        Some(spec) if spec.duration_ms > 0.0 => {
            decl.motion[index] = Some(AnimState::new(
                AnimSpec::Tween {
                    duration_ms: spec.duration_ms,
                    easing: spec.easing,
                },
                ExprValue::Color(decl.shown.color),
                ExprValue::Color(after),
                now_ms,
            ));
            decl.awaiting |= bit;
        }
        _ => {
            decl.shown.color = after;
            decl.motion[index] = None;
            decl.awaiting &= !bit;
        }
    }
}

fn chase_num(
    decl: &mut Decoration,
    index: usize,
    before: f32,
    after: f32,
    spec: Option<TransitionSpec>,
    now_ms: f64,
    radius: bool,
) {
    if before == after {
        return;
    }
    let bit = 1u8 << index;
    let shown = if radius {
        decl.shown.radius.unwrap_or(before)
    } else {
        shown_num(&decl.shown, index)
    };
    match spec {
        Some(spec) if spec.duration_ms > 0.0 => {
            decl.motion[index] = Some(AnimState::new(
                AnimSpec::Tween {
                    duration_ms: spec.duration_ms,
                    easing: spec.easing,
                },
                ExprValue::Num(shown),
                ExprValue::Num(after),
                now_ms,
            ));
            decl.awaiting |= bit;
        }
        _ => {
            write_shown(&mut decl.shown, index, ExprValue::Num(after));
            decl.motion[index] = None;
            decl.awaiting &= !bit;
        }
    }
}

fn shown_num(shown: &ResolvedDecoration, index: usize) -> f32 {
    match index {
        2 => shown.width,
        3 => shown.offset,
        4 => shown.offset_x,
        5 => shown.offset_y,
        6 => shown.blur,
        7 => shown.spread,
        _ => 0.0,
    }
}

fn write_shown(shown: &mut ResolvedDecoration, index: usize, value: ExprValue) {
    match (index, value) {
        (0, ExprValue::Color(color)) => shown.color = color,
        (1, ExprValue::Num(value)) => shown.radius = Some(value),
        (2, ExprValue::Num(value)) => shown.width = value,
        (3, ExprValue::Num(value)) => shown.offset = value,
        (4, ExprValue::Num(value)) => shown.offset_x = value,
        (5, ExprValue::Num(value)) => shown.offset_y = value,
        (6, ExprValue::Num(value)) => shown.blur = value,
        (7, ExprValue::Num(value)) => shown.spread = value,
        _ => {}
    }
}


fn resolve_slots(
    when: Option<DeclExpr>,
    color: Option<DeclExpr>,
    radius: Option<DeclExpr>,
    width: Option<DeclExpr>,
    offset: Option<DeclExpr>,
    offset_x: Option<DeclExpr>,
    offset_y: Option<DeclExpr>,
    blur: Option<DeclExpr>,
    spread: Option<DeclExpr>,
    vars: &VarTable,
) -> ResolvedDecoration {
    let mut resolved = ResolvedDecoration {
        matched: true,
        ..ResolvedDecoration::default()
    };
    if let Some(expr) = &when {
        match eval_typed(expr, Type::B, vars) {
            Ok(ExprValue::Bool(true)) => {}
            _ => return ResolvedDecoration::default(),
        }
    }
    if let Some(expr) = &color {
        match eval_typed(expr, Type::C, vars) {
            Ok(ExprValue::Color(color)) => resolved.color = color,
            _ => return ResolvedDecoration::default(),
        }
    }
    if let Some(expr) = &radius {
        match eval_typed(expr, Type::N, vars) {
            Ok(ExprValue::Num(value)) => resolved.radius = Some(value),
            _ => return ResolvedDecoration::default(),
        }
    }
    for (expr, slot) in [
        (width.as_ref(), &mut resolved.width),
        (offset.as_ref(), &mut resolved.offset),
        (offset_x.as_ref(), &mut resolved.offset_x),
        (offset_y.as_ref(), &mut resolved.offset_y),
        (blur.as_ref(), &mut resolved.blur),
        (spread.as_ref(), &mut resolved.spread),
    ] {
        if let Some(expr) = expr {
            match eval_typed(expr, Type::N, vars) {
                Ok(ExprValue::Num(value)) => *slot = value,
                _ => return ResolvedDecoration::default(),
            }
        }
    }
    resolved
}

fn eval_typed(expr: &DeclExpr, want: Type, vars: &VarTable) -> Result<ExprValue, ()> {
    let value = match expr {
        DeclExpr::Static(value) => *value,
        DeclExpr::Program(program) => program.eval(&|id| vars.get(id).map(|var| var.value)).map_err(|_| ())?,
    };
    let got = match value {
        ExprValue::Num(value) if value.is_finite() => Type::N,
        ExprValue::Num(_) => return Err(()),
        ExprValue::Color(_) => Type::C,
        ExprValue::Bool(_) => Type::B,
    };
    if got != want {
        return Err(());
    }
    Ok(value)
}

pub(crate) fn key_allowed(kind: u8, key: &str) -> bool {
    match key {
        "color" | "when" | "radius" | "layer" => true,
        "width" => kind == KIND_BORDER || kind == KIND_OUTLINE,
        "align" => kind == KIND_BORDER,
        "offset" => kind == KIND_OUTLINE,
        "offsetX" | "offsetY" | "blur" | "spread" => kind == KIND_SHADOW,
        _ => false,
    }
}

pub(crate) fn parse_layer(value: &str) -> Result<DecorationLayer, String> {
    match value {
        "background" => Ok(DecorationLayer::Background),
        "underlay" => Ok(DecorationLayer::Underlay),
        "overlay" => Ok(DecorationLayer::Overlay),
        other => Err(format!("unknown decoration layer {other}")),
    }
}

pub(crate) fn parse_align(value: &str) -> Result<BorderAlign, String> {
    match value {
        "inside" => Ok(BorderAlign::Inside),
        "center" => Ok(BorderAlign::Center),
        "outside" => Ok(BorderAlign::Outside),
        other => Err(format!("unknown decoration align {other}")),
    }
}

/// Expected program result for a programmable key.
pub(crate) fn slot_type(key: &str) -> Option<Type> {
    match key {
        "when" => Some(Type::B),
        "color" => Some(Type::C),
        "radius" | "width" | "offset" | "offsetX" | "offsetY" | "blur" | "spread" => Some(Type::N),
        _ => None,
    }
}

/// Static install check. `Program` is typed by the caller before this runs.
pub(crate) fn static_expr(key: &str, value: &crate::ops::Value) -> Result<Option<DeclExpr>, String> {
    if matches!(value, crate::ops::Value::Null) {
        if matches!(key, "color" | "layer" | "align") {
            return Err(format!("decoration {key} does not accept null"));
        }
        return Ok(None);
    }
    if matches!(value, crate::ops::Value::Program(_)) {
        return Err(format!("internal: {key} program must be typed first"));
    }
    let want = slot_type(key).ok_or_else(|| format!("decoration has no property {key}"))?;
    let parsed = match want {
        Type::B => ExprValue::Bool(value.as_bool().map_err(|error| format!("{key}: {error}"))?),
        Type::C => ExprValue::Color(value.as_color().map_err(|error| format!("{key}: {error}"))?),
        Type::N => {
            let number = value.as_f32().map_err(|error| format!("{key}: {error}"))?;
            if !number.is_finite() {
                return Err(format!("{key} must be a finite number"));
            }
            ExprValue::Num(number)
        }
    };
    Ok(Some(DeclExpr::Static(parsed)))
}

pub(crate) fn program_expr(program: Program) -> DeclExpr {
    DeclExpr::Program(Box::new(program))
}
