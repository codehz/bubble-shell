use crate::color::Color;
use crate::expr::ExprValue;
use crate::gesture::GestureEvent;
use crate::ops::Value;
use crate::state_style::{
    AnimSpec, AnimState, DeclExpr, ResolvedPatch, SlotType, VarEvent,
    VarTable, SOURCE_DISABLED, SOURCE_EDITING, SOURCE_FOCUSED, SOURCE_GESTURE_ROTATION,
    SOURCE_GESTURE_SCALE, SOURCE_GESTURE_TX, SOURCE_GESTURE_TY, SOURCE_NONE, SOURCE_PRESSED,
    SOURCE_SCROLL_X, SOURCE_SCROLL_Y,
};

use crate::ui::UiCall;
use super::paint::EffectivePaintStyle;
use super::tree::Node;
use super::Shell;

impl Shell {
    /// Base paint values with the currently matching style declaration
    /// patches applied. Reads cached declaration results only.
    pub(super) fn effective_paint_style(&self, id: u32, node: &Node) -> EffectivePaintStyle {
        let mut style = EffectivePaintStyle {
            background: node.background,
            color: node.color,
            opacity: node.opacity,
            radius: node.radius,
            text_align: node.text_align,
            fit: node.fit,
            translate_x: 0.0,
            translate_y: 0.0,
            scale: 1.0,
            rotate: 0.0,
        };
        let mut patch = ResolvedPatch::default();
        self.state_styles.overlay(id, &mut patch);
        if let Some(value) = patch.background {
            style.background = value;
        }
        if let Some(value) = patch.color {
            style.color = value;
        }
        if let Some(value) = patch.opacity {
            style.opacity = value;
        }
        if let Some(value) = patch.radius {
            style.radius = value;
        }
        if let Some(value) = patch.text_align {
            style.text_align = value;
        }
        if let Some(value) = patch.fit {
            style.fit = value;
        }
        if let Some(value) = patch.translate_x {
            style.translate_x = value;
        }
        if let Some(value) = patch.translate_y {
            style.translate_y = value;
        }
        if let Some(value) = patch.scale {
            style.scale = value;
        }
        if let Some(value) = patch.rotate {
            style.rotate = value;
        }
        style
    }

    /// Create a variable. Script-driven variables take their value from
    /// `initial`; source-driven ones start at the source's zero value and are
    /// attached to their target node by a later `BindVar`.
    pub(super) fn create_var(&mut self, var: u32, source: u8, initial: &Value) -> Result<(), String> {
        if source > SOURCE_SCROLL_Y {
            return Err(format!("var {var} has unknown source {source}"));
        }
        let value = match source {
            SOURCE_NONE => parse_expr_value(initial).ok_or_else(|| {
                format!("var {var} initial value must be a number, color, or bool")
            })?,
            SOURCE_PRESSED
            | SOURCE_GESTURE_TX
            | SOURCE_GESTURE_TY
            | SOURCE_GESTURE_ROTATION
            | SOURCE_SCROLL_X
            | SOURCE_SCROLL_Y => ExprValue::Num(0.0),
            // Pinch scale reads as "no transform" while idle.
            SOURCE_GESTURE_SCALE => ExprValue::Num(1.0),
            _ => ExprValue::Bool(false),
        };
        self.vars.create(var, source, 0, value)
    }

    /// Attach a variable's source target. Re-samples the source so dependent
    /// declarations re-evaluate.
    pub(super) fn bind_var(&mut self, var: u32, target: u32) -> Result<(), String> {
        let Some(var_slot) = self.vars.get_mut(var) else {
            return Err(format!("unknown var {var}"));
        };
        var_slot.target = target;
        self.sample_sources();
        Ok(())
    }

    pub(super) fn dispose_var(&mut self, var: u32) -> Result<(), String> {
        let dependents = self.vars.dispose(var)?;
        for decl in dependents {
            self.mark_decl_unmatched(decl);
        }
        self.paint_dirty.set(true);
        Ok(())
    }

    /// Apply one script-driven variable write. Source-linked variables are
    /// owned by the UI thread, so the script may not write them. `anim`
    /// carries a tween or spring started from the current displayed value;
    /// `None` lands immediately.
    pub(super) fn set_var(&mut self, var: u32, value: &Value, anim: Option<AnimSpec>) -> Result<(), String> {
        let next = parse_expr_value(value)
            .ok_or_else(|| format!("var {var} value must be a number, color, or bool"))?;
        let Some(var_slot) = self.vars.get_mut(var) else {
            return Err(format!("unknown var {var}"));
        };
        if var_slot.source != SOURCE_NONE {
            return Err(format!("var {var} is driven by a state source"));
        }
        if var_slot.value == next {
            var_slot.anim_state = None;
            return Ok(());
        }
        // Bool animates instantly, and so does anything the engine cannot
        // interpolate; only Num and Color transitions actually run.
        let animatable =
            matches!(next, ExprValue::Num(_) | ExprValue::Color(_)) && anim.is_some();
        if animatable {
            let from = var_slot.value.clone();
            var_slot.anim_state = Some(AnimState::new(anim.expect("checked"), from, next, self.now_ms));
        } else {
            var_slot.anim_state = None;
            var_slot.value = next;
            self.mark_var_dirty(var);
        }
        Ok(())
    }

    /// Stop a variable's in-flight animation; the value stays where it is.
    pub(super) fn cancel_var_anim(&mut self, var: u32) -> Result<(), String> {
        let Some(var_slot) = self.vars.get_mut(var) else {
            return Err(format!("unknown var {var}"));
        };
        var_slot.anim_state = None;
        Ok(())
    }

    /// Queue a variable's dependents for re-evaluation at the end of the
    /// current UI batch. One pass per batch instead of one per write.
    pub(crate) fn mark_var_dirty(&mut self, var: u32) {
        self.ui_dirty.insert(var);
        self.paint_dirty.set(true);
    }

    /// Re-evaluate every declaration that reads a variable written in this
    /// UI batch, then flush settled-variable events.
    pub(super) fn reval_dirty(&mut self) {
        if self.ui_dirty.is_empty() {
            return;
        }
        let ids: Vec<u32> = self.ui_dirty.drain().collect();
        for decl in self.state_styles.dependents_of(ids.iter().copied()) {
            self.eval_decl(decl);
        }
        for id in ids {
            self.flush_var_settle(id);
        }
    }

    /// Record a settle request. The event goes out when the variable
    /// converges: immediately for instant writes, at the animation tick for
    /// in-flight ones.
    pub(super) fn settle_var(&mut self, var: u32) -> Result<(), String> {
        if self.vars.get(var).is_none() {
            return Err(format!("unknown var {var}"));
        }
        if !self.pending_settle.insert(var) {
            return Ok(());
        }
        self.flush_var_settle(var);
        Ok(())
    }

    /// Emit the settle event if `var` has a pending request and no in-flight
    /// animation.
    fn flush_var_settle(&mut self, var: u32) {
        let converged = self
            .vars
            .get(var)
            .is_some_and(|var| var.anim_state.is_none());
        if converged && self.pending_settle.remove(&var) {
            self.var_events.push(VarEvent { var });
        }
    }

    /// Integrate in-flight variable transitions at the tick. Changed values
    /// re-evaluate the declarations that read them; convergence flushes
    /// pending settle requests.
    pub fn advance_animations(&mut self, now_ms: f64) {
        self.now_ms = now_ms;
        let ids: Vec<u32> = self
            .vars
            .all_with_anim()
            .into_iter()
            .filter(|id| {
                self.vars
                    .get(*id)
                    .is_some_and(|var| var.anim_state.is_some())
            })
            .collect();
        if ids.is_empty() {
            return;
        }
        let mut changed: Vec<u32> = Vec::new();
        for id in ids {
            let Some(var_slot) = self.vars.get_mut(id) else {
                continue;
            };
            let Some(state) = var_slot.anim_state.as_mut() else {
                continue;
            };
            let (next, converged) = state.step(now_ms);
            if let Some(next) = next {
                if var_slot.value != next {
                    var_slot.value = next;
                    changed.push(id);
                }
            }
            let to = state.to.clone();
            if converged {
                var_slot.anim_state = None;
                if var_slot.value != to {
                    var_slot.value = to;
                    changed.push(id);
                }
            }
            self.flush_var_settle(id);
        }
        if changed.is_empty() {
            return;
        }
        for decl in self.state_styles.dependents_of(changed) {
            self.eval_decl(decl);
        }
        self.paint_dirty.set(true);
    }

    /// Settled-variable events since the last drain, for the script thread.
    pub fn take_var_events(&mut self) -> Vec<VarEvent> {
        std::mem::take(&mut self.var_events)
    }

    /// Mark one declaration as not matching after a variable it read retired.
    pub(super) fn mark_decl_unmatched(&mut self, decl: u32) {
        if let Some(decl) = self.state_styles_decl_mut(decl) {
            decl.resolved_when = false;
            decl.resolved = ResolvedPatch::default();
        }
        self.paint_dirty.set(true);
    }

    fn state_styles_decl_mut(&mut self, id: u32) -> Option<&mut crate::state_style::StyleDecl> {
        // The declaration table is owned by `state_styles`; expose the mutable
        // decl through a narrow accessor on the table.
        self.state_styles.decl_mut(id)
    }

    /// Re-evaluate one declaration's cached result against current variable
    /// values. Programs that fail re-evaluation mark the declaration as not
    /// matching rather than corrupting paint.
    pub(super) fn eval_decl(&mut self, decl: u32) {
        let (when, patch) = {
            let Some(decl) = self.state_styles.decl(decl) else {
                return;
            };
            (decl.when.clone(), decl.patch.clone())
        };
        let mut resolved = ResolvedPatch::default();
        let mut matched = match &when {
            Some(when) => {
                matches!(
                    decl_eval_slot(None, when, &self.vars),
                    Ok(Some(ExprValue::Bool(true)))
                )
            }
            None => false,
        };
        if matched {
            for slot in 0..crate::state_style::SLOT_COUNT {
                let Some(expr) = &patch[slot] else {
                    continue;
                };
                let value = match decl_eval_slot(Some(slot), expr, &self.vars) {
                    Ok(Some(value)) => value,
                    _ => {
                        matched = false;
                        break;
                    }
                };
                resolved = match (value, crate::state_style::slot_type(slot)) {
                    (ExprValue::Color(color), SlotType::Color) => {
                        assign_resolved(resolved, slot, color, 0.0)
                    }
                    (ExprValue::Num(code), SlotType::Num | SlotType::Enum) => {
                        assign_resolved(resolved, slot, Color::TRANSPARENT, code)
                    }
                    _ => {
                        matched = false;
                        break;
                    }
                };
            }
        }
        let Some(decl) = self.state_styles.decl_mut(decl) else {
            return;
        };
        decl.resolved_when = matched;
        decl.resolved = resolved;
    }

    /// Re-sample every source-driven variable and re-evaluate the
    /// declarations that read a changed one. Cheap: only vars with sources,
    /// and only changed vars touch declarations.
    pub(super) fn sample_sources(&mut self) {
        let mut changed: Vec<u32> = Vec::new();
        let ids: Vec<(u32, u8, u32)> = self.vars.all_with_source();
        for (id, source, target) in ids {
            let next = match source {
                SOURCE_PRESSED => ExprValue::Num(if self.visual.target == target {
                    1.0
                } else {
                    0.0
                }),
                SOURCE_FOCUSED => ExprValue::Bool(self.focused == target && target != 0),
                SOURCE_DISABLED => {
                    ExprValue::Bool(self.node(target).map(|node| node.disabled).unwrap_or(true))
                }
                SOURCE_EDITING => ExprValue::Bool(
                    target != 0 && self.focused == target && self.fields.contains_key(&target),
                ),
                SOURCE_GESTURE_TX
                | SOURCE_GESTURE_TY
                | SOURCE_GESTURE_SCALE
                | SOURCE_GESTURE_ROTATION => ExprValue::Num(self.recognizer.live(target, source)),
                SOURCE_SCROLL_X => ExprValue::Num(
                    self.node(target)
                        .map(|node| node.scroll_left)
                        .unwrap_or(0.0),
                ),
                SOURCE_SCROLL_Y => {
                    ExprValue::Num(self.node(target).map(|node| node.scroll_top).unwrap_or(0.0))
                }
                _ => continue,
            };
            if let Some(var) = self.vars.get_mut(id) {
                if var.value != next {
                    var.value = next;
                    changed.push(id);
                }
            }
        }
        if changed.is_empty() {
            return;
        }
        for decl in self.state_styles.dependents_of(changed) {
            self.eval_decl(decl);
        }
        self.paint_dirty.set(true);
    }

    /// A matching `focused` declaration replaces the default focus ring, even
    /// when it patches nothing. Disabled or not-yet-matching ones do not.
    pub(super) fn focus_style_taken(&self, id: u32) -> bool {
        self.state_styles.focus_style_taken(id)
    }

    /// Register one UI program ("worklet"). Validation fails the batch.
    pub(super) fn define_ui_function(
        &mut self,
        id: u32,
        params: u8,
        captures: Vec<crate::ui::UiCapture>,
        consts: Vec<crate::expr::ExprConst>,
        code: Vec<u8>,
    ) -> Result<(), String> {
        let program = crate::ui::UiProgram {
            params,
            captures,
            consts,
            code,
        };
        program.validate()?;
        if id == 0 {
            return Err("ui function id must be nonzero".into());
        }
        if self.ui_programs.get(id as usize).is_some_and(Option::is_some) {
            return Err(format!("ui function {id} is not free"));
        }
        if self.ui_programs.len() <= id as usize {
            self.ui_programs.resize_with(id as usize + 1, || None);
        }
        self.ui_programs[id as usize] = Some(program);
        Ok(())
    }

    /// Run every queued UI call: gesture worklets first, then JS-initiated
    /// calls, then one dependent re-evaluation pass. A failing call records
    /// a fault and continues.
    pub(crate) fn drain_ui_work(&mut self) {
        let gesture_events: Vec<(u32, GestureEvent)> = self.recognizer.take_ui_events();
        let calls: Vec<UiCall> = self.ui_calls.drain(..).collect();
        if gesture_events.is_empty() && calls.is_empty() {
            return;
        }
        for (id, event) in gesture_events {
            self.ui_event = Some(event);
            let result = self.run_ui_function(id, &[]);
            self.ui_event = None;
            let _ = result;
        }
        for call in calls {
            self.ui_event = None;
            let mut args = Vec::with_capacity(call.args.len());
            let mut ok = true;
            for arg in call.args {
                match arg.as_expr() {
                    Ok(value) => args.push(value),
                    Err(error) => {
                        self.ui_faults += 1;
                        self.ui_last_error = error;
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                let _ = self.run_ui_function(call.function, &args);
            }
        }
        self.reval_dirty();
    }
}

/// Static value for a variable or patch slot. Numeric colors are
/// `0xAARRGGBB`; strings parse as CSS colors.
pub(super) fn parse_expr_value(value: &Value) -> Option<ExprValue> {
    match value {
        Value::Bool(value) => Some(ExprValue::Bool(*value)),
        Value::Int(value) => Some(ExprValue::Num(*value as f32)),
        Value::Float(value) if value.is_finite() => Some(ExprValue::Num(*value as f32)),
        _ => value.as_color().ok().map(ExprValue::Color),
    }
}

/// Evaluate one declaration slot against current variable values. The
/// apply-time pass runs this first, so type or range errors fail the batch.
fn decl_eval_slot(
    slot: Option<usize>,
    expr: &DeclExpr,
    vars: &VarTable,
) -> Result<Option<ExprValue>, String> {
    let value = match expr {
        DeclExpr::Static(value) => *value,
        DeclExpr::Program(program) => program
            .eval(&|id| vars.get(id).map(|var| var.value))
            .map_err(|error| error.to_string())?,
    };
    match slot {
        None => match value {
            ExprValue::Bool(_) => Ok(Some(value)),
            _ => Err("when must be a bool".into()),
        },
        Some(slot) => match crate::state_style::slot_type(slot) {
            SlotType::Num => match value {
                ExprValue::Num(value) if value.is_finite() => Ok(Some(ExprValue::Num(value))),
                _ => Err("patch expects a finite number".into()),
            },
            SlotType::Color => match value {
                ExprValue::Color(_) => Ok(Some(value)),
                _ => Err("patch expects a color".into()),
            },
            SlotType::Enum => match value {
                ExprValue::Num(value) if value.is_finite() => {
                    let code = value.round();
                    if (0.0..=2.0).contains(&value) && (value - code).abs() <= f32::EPSILON * 4.0 {
                        Ok(Some(ExprValue::Num(code)))
                    } else {
                        Err("patch expects a textAlign or fit code in 0..=2".into())
                    }
                }
                _ => Err("patch expects a textAlign or fit code in 0..=2".into()),
            },
        },
    }
}

/// Fold one evaluated slot into a resolved patch.
fn assign_resolved(mut patch: ResolvedPatch, slot: usize, color: Color, num: f32) -> ResolvedPatch {
    match slot {
        crate::state_style::SLOT_BACKGROUND => patch.background = Some(color),
        crate::state_style::SLOT_COLOR => patch.color = Some(color),
        crate::state_style::SLOT_OPACITY => patch.opacity = Some(num.clamp(0.0, 1.0)),
        crate::state_style::SLOT_RADIUS => patch.radius = Some(num.max(0.0)),
        crate::state_style::SLOT_TEXT_ALIGN => patch.text_align = Some(num as u8),
        crate::state_style::SLOT_FIT => patch.fit = Some(num as u8),
        crate::state_style::SLOT_TRANSLATE_X => patch.translate_x = Some(num),
        crate::state_style::SLOT_TRANSLATE_Y => patch.translate_y = Some(num),
        crate::state_style::SLOT_SCALE => patch.scale = Some(num),
        crate::state_style::SLOT_ROTATE => patch.rotate = Some(num),
        _ => {}
    }
    patch
}
