//! Virtual state style declarations.
//!
//! A declaration is not a node: it owns no box, never hits, never paints, and
//! stays out of Taffy and the semantic tree. It carries one conditional patch
//! of paint properties for a real node. Declarations are ordered per target and
//! applied in that order, so a later matching declaration wins. Conditions are
//! a set of state bits ANDed together.

use crate::color::Color;
use crate::ops::Value;
use crate::ordered::OrderedDecls;
use crate::props::{image_fit, text_align};

/// Condition bit: the first eligible pointer is down on the target.
pub(crate) const WHEN_PRESSED: u8 = 1;
/// Condition bit: the target holds UI focus.
pub(crate) const WHEN_FOCUSED: u8 = 2;

/// Resolved paint overrides. `None` leaves the node's base value visible.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct StateStylePatch {
    pub background: Option<Color>,
    pub color: Option<Color>,
    pub opacity: Option<f32>,
    pub radius: Option<f32>,
    pub text_align: Option<u8>,
    pub fit: Option<u8>,
}

impl StateStylePatch {
    /// Parse one key. Null clears that entry so the node base shows through.
    fn set(&mut self, key: &str, value: &Value) -> Result<(), String> {
        match key {
            "backgroundColor" => self.background = optional(value, Value::as_color)?,
            "color" => self.color = optional(value, Value::as_color)?,
            "opacity" => {
                self.opacity = optional(value, |value| {
                    value.as_f32().map(|number| number.clamp(0.0, 1.0))
                })?
            }
            "radius" => self.radius = optional(value, Value::as_f32)?,
            "textAlign" => self.text_align = optional(value, text_align)?,
            "fit" => self.fit = optional(value, image_fit)?,
            _ => return Err(format!("state style has no property {key}")),
        }
        Ok(())
    }

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
    }
}

/// `Null` means "no override"; anything else is parsed.
fn optional<T>(
    value: &Value,
    parse: impl Fn(&Value) -> Result<T, String>,
) -> Result<Option<T>, String> {
    match value {
        Value::Null => Ok(None),
        other => parse(other).map(Some),
    }
}

/// Whitespace-split tokens, ANDed. Duplicates merge.
fn parse_when(value: &Value) -> Result<u8, String> {
    let Value::String(text) = value else {
        return Err("state style when must be a string".into());
    };
    let mut bits = 0u8;
    for token in text.split_whitespace() {
        bits |= match token {
            "pressed" => WHEN_PRESSED,
            "focused" => WHEN_FOCUSED,
            other => return Err(format!("unknown state style condition {other}")),
        };
    }
    if bits == 0 {
        return Err("state style when is empty".into());
    }
    Ok(bits)
}

#[derive(Clone, Debug)]
pub(crate) struct StateStyleDecl {
    pub target: u32,
    /// Condition bits. `0` until the author runtime configures `when`, which
    /// matches no state.
    pub when: u8,
    pub enabled: bool,
    pub patch: StateStylePatch,
}

/// Declarations plus per-target order. Ids are independent of node handles.
pub(crate) struct StateStyles {
    ordered: OrderedDecls<StateStyleDecl>,
}

impl StateStyles {
    pub fn new() -> Self {
        Self {
            ordered: OrderedDecls::new(),
        }
    }

    pub fn decl(&self, id: u32) -> Option<&StateStyleDecl> {
        self.ordered.decl(id)
    }

    fn decl_mut(&mut self, id: u32) -> Option<&mut StateStyleDecl> {
        self.ordered.decl_mut(id)
    }

    pub fn spawn(&mut self, id: u32, target: u32, index: u32) -> Result<(), String> {
        self.ordered.insert(
            id,
            target,
            index,
            "state style",
            StateStyleDecl {
                target,
                when: 0,
                enabled: true,
                patch: StateStylePatch::default(),
            },
        )
    }

    /// Apply one property. The value is parsed before the declaration changes.
    pub fn set_prop(&mut self, id: u32, key: &str, value: &Value) -> Result<(), String> {
        let decl = self
            .decl_mut(id)
            .ok_or_else(|| format!("unknown state style {id}"))?;
        match key {
            "when" => decl.when = parse_when(value)?,
            "enabled" => decl.enabled = value.as_bool()?,
            other => decl.patch.set(other, value)?,
        }
        Ok(())
    }

    /// `index` is the final position after the declaration leaves its old slot.
    pub fn move_decl(&mut self, id: u32, target: u32, index: u32) -> Result<(), String> {
        let old_target = self
            .decl(id)
            .ok_or_else(|| format!("unknown state style {id}"))?
            .target;
        self.ordered
            .move_decl(id, old_target, target, index, "state style")?;
        if let Some(decl) = self.decl_mut(id) {
            decl.target = target;
        }
        Ok(())
    }

    pub fn dispose(&mut self, id: u32) -> Result<(), String> {
        let target = self
            .decl(id)
            .ok_or_else(|| format!("unknown state style {id}"))?
            .target;
        self.ordered.remove(id, target);
        Ok(())
    }

    /// Drop every declaration attached to a destroyed node.
    pub fn target_disposed(&mut self, target: u32) {
        self.ordered.retire_target(target);
    }

    /// Merge the enabled declarations on `target` that fully match `state`.
    pub fn overlay(&self, target: u32, state: u8, out: &mut StateStylePatch) {
        let Some(ids) = self.ordered.ids(target) else {
            return;
        };
        for id in ids {
            let Some(decl) = self.decl(*id) else {
                continue;
            };
            if decl.enabled && decl.when != 0 && decl.when & state == decl.when {
                out.apply(&decl.patch);
            }
        }
    }

    /// Whether a declaration that reacts to a bit of `state` currently matches.
    /// Used to decide whether the default focus ring is taken over.
    pub fn matching(&self, target: u32, bit: u8, state: u8) -> bool {
        let Some(ids) = self.ordered.ids(target) else {
            return false;
        };
        ids.iter().any(|id| {
            self.decl(*id).is_some_and(|decl| {
                decl.enabled && decl.when & bit != 0 && decl.when & state == decl.when
            })
        })
    }

    /// Whether `target` still wants pressed feedback: any enabled declaration
    /// with the pressed bit, whether or not the other conditions hold.
    pub fn pressed_interest(&self, target: u32) -> bool {
        let Some(ids) = self.ordered.ids(target) else {
            return false;
        };
        ids.iter().any(|id| {
            self.decl(*id)
                .is_some_and(|decl| decl.enabled && decl.when & WHEN_PRESSED != 0)
        })
    }
}
