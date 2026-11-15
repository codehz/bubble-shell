//! Ordered slots for virtual child declarations.
//!
//! Gestures and state styles are both virtual children: they attach to a real
//! node, never lay out, and are ordered per target. This owns the id slots, the
//! per-target order, and retirement; each registry keeps only its payload and
//! its own validation.

use std::collections::{HashMap, HashSet};

pub(crate) struct OrderedDecls<T> {
    pub(crate) decls: Vec<Option<T>>,
    pub(crate) order: HashMap<u32, Vec<u32>>,
    pub(crate) retired: HashSet<u32>,
}

impl<T> OrderedDecls<T> {
    pub fn new() -> Self {
        Self {
            decls: Vec::new(),
            order: HashMap::new(),
            retired: HashSet::new(),
        }
    }

    pub fn decl(&self, id: u32) -> Option<&T> {
        self.decls.get(id as usize).and_then(Option::as_ref)
    }

    pub fn decl_mut(&mut self, id: u32) -> Option<&mut T> {
        self.decls.get_mut(id as usize).and_then(Option::as_mut)
    }

    pub fn ids(&self, target: u32) -> Option<&[u32]> {
        self.order.get(&target).map(Vec::as_slice)
    }

    /// Claim `id` for `target` at authored position `index`.
    pub fn insert(
        &mut self,
        id: u32,
        target: u32,
        index: u32,
        what: &str,
        value: T,
    ) -> Result<(), String> {
        if id == 0 || self.retired.contains(&id) || self.decl(id).is_some() {
            return Err(format!("{what} {id} is not free"));
        }
        let list = self.order.entry(target).or_default();
        if index as usize > list.len() {
            return Err(format!(
                "{what} index {index} past {} declarations",
                list.len()
            ));
        }
        list.insert(index as usize, id);
        if self.decls.len() <= id as usize {
            self.decls.resize_with(id as usize + 1, || None);
        }
        self.decls[id as usize] = Some(value);
        Ok(())
    }

    /// Move `id` from `old_target` to `target`. `index` is the final position
    /// after the declaration leaves its old slot. The payload's target is the
    /// caller's to update.
    pub fn move_decl(
        &mut self,
        id: u32,
        old_target: u32,
        target: u32,
        index: u32,
        what: &str,
    ) -> Result<(), String> {
        let len = self.order.get(&target).map_or(0, Vec::len);
        let held = old_target == target
            && self
                .order
                .get(&target)
                .is_some_and(|list| list.contains(&id));
        let capacity = len - usize::from(held);
        if index as usize > capacity {
            return Err(format!("{what} index {index} past {capacity} declarations"));
        }
        if let Some(list) = self.order.get_mut(&old_target) {
            list.retain(|existing| *existing != id);
            if list.is_empty() {
                self.order.remove(&old_target);
            }
        }
        self.order
            .entry(target)
            .or_default()
            .insert(index as usize, id);
        Ok(())
    }

    pub fn remove(&mut self, id: u32, target: u32) {
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
    }

    /// Drop every declaration on a destroyed target and return their ids.
    pub fn retire_target(&mut self, target: u32) -> Vec<u32> {
        let ids = self.order.remove(&target).unwrap_or_default();
        for id in &ids {
            if self.decls.len() > *id as usize {
                self.decls[*id as usize] = None;
            }
            self.retired.insert(*id);
        }
        ids
    }
}
