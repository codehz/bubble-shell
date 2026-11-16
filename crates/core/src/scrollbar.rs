//! Virtual scrollbar declarations.
//!
//! A scrollbar attaches to a real node by handle, keeps a per-target order, and
//! never enters taffy. Thumb geometry comes from the target's box and scroll
//! state, not from a layout child.

use crate::color::Color;
use crate::ordered::OrderedDecls;

pub(crate) struct ScrollBar {
    pub(crate) target: u32,
    /// `0` vertical, anything else horizontal. Both axes are two declarations.
    pub(crate) axis: u8,
    pub(crate) width: f32,
    pub(crate) margin: f32,
    pub(crate) color: Color,
    pub(crate) radius: f32,
    pub(crate) opacity: f32,
    pub(crate) min_length: f32,
    pub(crate) interactive: bool,
    pub(crate) auto_hide: bool,
}

impl ScrollBar {
    fn default(target: u32) -> Self {
        Self {
            target,
            axis: 0,
            width: 4.0,
            margin: 2.0,
            color: Color::new(1.0, 1.0, 1.0, 0.5),
            radius: -1.0,
            opacity: 0.5,
            min_length: 20.0,
            interactive: true,
            auto_hide: true,
        }
    }
}

pub(crate) struct ScrollBarDecls {
    ordered: OrderedDecls<ScrollBar>,
}

impl ScrollBarDecls {
    pub(crate) fn new() -> Self {
        Self {
            ordered: OrderedDecls::new(),
        }
    }

    pub(crate) fn scrollbar(&self, id: u32) -> Option<&ScrollBar> {
        self.ordered.decl(id)
    }

    pub(crate) fn scrollbar_mut(&mut self, id: u32) -> Option<&mut ScrollBar> {
        self.ordered.decl_mut(id)
    }

    pub(crate) fn ids(&self, target: u32) -> Option<&[u32]> {
        self.ordered.ids(target)
    }

    pub(crate) fn decl_slots(&self) -> usize {
        self.ordered.decls.len()
    }

    pub(crate) fn at(&self, index: usize) -> Option<&ScrollBar> {
        self.ordered.decls.get(index).and_then(Option::as_ref)
    }

    pub(crate) fn spawn(&mut self, id: u32, target: u32, index: u32) -> Result<(), String> {
        self.ordered
            .insert(id, target, index, "scrollbar", ScrollBar::default(target))
    }

    pub(crate) fn move_decl(&mut self, id: u32, target: u32, index: u32) -> Result<(), String> {
        let old_target = self
            .ordered
            .decl(id)
            .ok_or_else(|| format!("unknown scrollbar {id}"))?
            .target;
        self.ordered
            .move_decl(id, old_target, target, index, "scrollbar")?;
        if let Some(bar) = self.ordered.decl_mut(id) {
            bar.target = target;
        }
        Ok(())
    }

    pub(crate) fn dispose(&mut self, id: u32) -> Result<(), String> {
        let target = self
            .ordered
            .decl(id)
            .ok_or_else(|| format!("unknown scrollbar {id}"))?
            .target;
        self.ordered.remove(id, target);
        Ok(())
    }

    pub(crate) fn target_disposed(&mut self, target: u32) -> Vec<u32> {
        self.ordered.retire_target(target)
    }
}
