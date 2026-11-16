use crate::ir::Kind;
use crate::semantics::{
    Role, SemanticsNode, SemanticsSnapshot, ACTION_ACTIVATE, ACTION_FOCUS, ACTION_SCROLL_BACKWARD,
    ACTION_SCROLL_DOWN, ACTION_SCROLL_FORWARD, ACTION_SCROLL_LEFT, ACTION_SCROLL_RIGHT,
    ACTION_SCROLL_UP, ACTION_SET_VALUE,
};

use super::tree::Node;
use super::{page_step, A11yAction, ScrollAxis, Shell};

impl Shell {
    /// Semantic snapshot in viewport coordinates. Nodes are in pre-order;
    /// `parent` points at the nearest semantic ancestor.
    pub fn semantics(&self) -> SemanticsSnapshot {
        let modal = self.active_modal();
        let mut nodes = Vec::new();
        self.collect_semantics(1, 0, 0, false, modal, &mut nodes);
        SemanticsSnapshot {
            version: 1,
            focused: self.focused,
            modal,
            nodes,
        }
    }

    pub fn semantics_json(&self) -> String {
        self.semantics().to_json()
    }

    fn collect_semantics(
        &self,
        id: u32,
        parent: u32,
        depth: u32,
        hidden: bool,
        modal: u32,
        out: &mut Vec<SemanticsNode>,
    ) {
        let Ok(node) = self.node(id) else {
            return;
        };
        if node.display_none {
            return;
        }
        let outside = modal != 0 && !self.contains(modal, id);
        let inherited_hidden = hidden || node.a11y_hidden || node.visibility_hidden;
        let hidden = inherited_hidden || outside;
        // A modal must not remain under an aria-hidden/background ancestor.
        let (parent, depth) = if id == modal { (0, 0) } else { (parent, depth) };
        let role = self.node_role(node);
        let semantic = node.kind != Kind::View
            || node.role.is_some()
            || !node.label.is_empty()
            || !node.test_id.is_empty()
            || node.scrolls_x()
            || node.scrolls_y()
            || self.is_focusable(id)
            || node.modal
            || node.disabled;
        let mut next_parent = parent;
        let mut next_depth = depth;
        if semantic {
            let mut actions = 0;
            if self.is_focusable(id) {
                actions |= ACTION_FOCUS;
            }
            if self.recognizer.has_tap(id) {
                actions |= ACTION_ACTIVATE;
            }
            if node.kind == Kind::Field {
                actions |= ACTION_SET_VALUE;
            }
            if node.scrolls_x() {
                actions |= ACTION_SCROLL_FORWARD
                    | ACTION_SCROLL_BACKWARD
                    | ACTION_SCROLL_LEFT
                    | ACTION_SCROLL_RIGHT;
            }
            if node.scrolls_y() {
                actions |= ACTION_SCROLL_FORWARD
                    | ACTION_SCROLL_BACKWARD
                    | ACTION_SCROLL_UP
                    | ACTION_SCROLL_DOWN;
            }
            if hidden || node.disabled {
                actions = 0;
            }
            let editable = node.kind == Kind::Field;
            let secure = editable && node.secure;
            let (label, value) = match node.kind {
                Kind::Text => (
                    if node.label.is_empty() {
                        node.text.clone()
                    } else {
                        node.label.clone()
                    },
                    String::new(),
                ),
                Kind::Field => (
                    node.label.clone(),
                    if secure {
                        String::new()
                    } else {
                        node.text.clone()
                    },
                ),
                _ => (node.label.clone(), String::new()),
            };
            out.push(SemanticsNode {
                handle: id,
                parent,
                depth,
                role,
                label,
                value,
                test_id: node.test_id.clone(),
                bounds: [node.x, node.y, node.w, node.h],
                actions,
                focusable: !hidden && !node.disabled && self.is_focusable(id),
                focused: id == self.focused,
                disabled: node.disabled,
                hidden,
                checked: node.checked,
                selected: node.selected,
                expanded: node.expanded,
                secure,
                editable,
                scrollable: node.scrolls_x() || node.scrolls_y(),
            });
            next_parent = id;
            next_depth = depth + 1;
        }
        let inherited = inherited_hidden;
        let children = node.children.clone();
        for child in children {
            self.collect_semantics(child, next_parent, next_depth, inherited, modal, out);
        }
    }

    fn node_role(&self, node: &Node) -> Role {
        node.role.unwrap_or(match node.kind {
            Kind::Text => Role::Text,
            Kind::Field => Role::Textbox,
            Kind::Image => Role::Image,
            Kind::View | Kind::ScrollBar => Role::Generic,
        })
    }

    /// Route a screen-reader action to a semantic node.
    pub fn a11y_action(&mut self, handle: u32, action: A11yAction) -> Result<(), String> {
        self.node(handle)?;
        if !self.interaction_available(handle, self.active_modal()) {
            return Err(format!("handle {handle} is unavailable"));
        }
        match action {
            A11yAction::Focus => self.focus(handle),
            A11yAction::Activate => {
                if self.node(handle)?.kind == Kind::Field {
                    self.focus(handle)
                } else {
                    let (cx, cy) = {
                        let node = self.node(handle)?;
                        (node.x + node.w / 2.0, node.y + node.h / 2.0)
                    };
                    self.recognizer.activate(handle, cx, cy, 0.0);
                    Ok(())
                }
            }
            A11yAction::SetValue(text) => self.set_field_text(handle, &text),
            action @ (A11yAction::ScrollForward
            | A11yAction::ScrollBackward
            | A11yAction::ScrollLeft
            | A11yAction::ScrollRight
            | A11yAction::ScrollUp
            | A11yAction::ScrollDown) => self.a11y_scroll(handle, action),
        }
    }

    /// Scroll the nearest ancestor that accepts the requested axis. The step
    /// is a page of the visible extent, so platform bridges never invent their
    /// own pixel deltas.
    fn a11y_scroll(&mut self, handle: u32, action: A11yAction) -> Result<(), String> {
        let (target, axis) = match action {
            A11yAction::ScrollForward | A11yAction::ScrollBackward => {
                let target = self.scroll_target(handle, ScrollAxis::Primary)?;
                let node = self.node(target)?;
                (
                    target,
                    if node.scrolls_y() {
                        ScrollAxis::Y
                    } else {
                        ScrollAxis::X
                    },
                )
            }
            A11yAction::ScrollLeft | A11yAction::ScrollRight => {
                (self.scroll_target(handle, ScrollAxis::X)?, ScrollAxis::X)
            }
            A11yAction::ScrollUp | A11yAction::ScrollDown => {
                (self.scroll_target(handle, ScrollAxis::Y)?, ScrollAxis::Y)
            }
            _ => unreachable!("a11y_scroll only handles scroll actions"),
        };
        let (w, h) = {
            let node = self.node(target)?;
            (node.w, node.h)
        };
        let step_x = page_step(w);
        let step_y = page_step(h);
        let (dx, dy) = match action {
            A11yAction::ScrollForward => {
                if axis == ScrollAxis::Y {
                    (0.0, step_y)
                } else {
                    (step_x, 0.0)
                }
            }
            A11yAction::ScrollBackward => {
                if axis == ScrollAxis::Y {
                    (0.0, -step_y)
                } else {
                    (-step_x, 0.0)
                }
            }
            A11yAction::ScrollLeft => (-step_x, 0.0),
            A11yAction::ScrollRight => (step_x, 0.0),
            A11yAction::ScrollUp => (0.0, -step_y),
            A11yAction::ScrollDown => (0.0, step_y),
            _ => unreachable!("a11y_scroll only handles scroll actions"),
        };
        self.scroll_path(&[target], dx, dy);
        Ok(())
    }

    fn scroll_target(&self, handle: u32, axis: ScrollAxis) -> Result<u32, String> {
        let mut id = handle;
        let modal = self.active_modal();
        while id != 0 {
            if !self.interaction_available(id, modal) {
                break;
            }
            let node = self.node(id)?;
            let accepts = match axis {
                ScrollAxis::X => node.scrolls_x(),
                ScrollAxis::Y => node.scrolls_y(),
                ScrollAxis::Primary => node.scrolls_x() || node.scrolls_y(),
            };
            if accepts {
                return Ok(id);
            }
            id = node.parent;
        }
        Err(format!("handle {handle} has no scrollable ancestor"))
    }
}
