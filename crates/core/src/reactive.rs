#![allow(dangerous_implicit_autorefs)]
//! Solid 1.9 scheduling without stored values.
//!
//! The host keeps each value and reports whether a memo changed. The graph
//! tracks sources, owners, and the update and effect queues. Callbacks run
//! only while the graph is not borrowed, so the host may re-enter through
//! the same `*mut Graph`.

const NONE: u32 = u32::MAX;
const DISPOSED_OWNER: u32 = u32::MAX - 2;
const CLEAN: u8 = 0;
const STALE: u8 = 1;
const PENDING: u8 = 2;
const MAX_RUNS: u64 = 1_000_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Signal,
    Memo,
    Effect,
    UserEffect,
}

pub trait Sched {
    fn body(&mut self) {}
    /// `1` memo changed, `0` unchanged, negative failed.
    fn run(&mut self, id: u32) -> i32;
    fn cleanup(&mut self, id: u32);
}

struct Node {
    kind: Kind,
    state: u8,
    sources: Vec<(u32, u32)>,
    observers: Vec<(u32, u32)>,
    owner: u32,
    owned: Vec<u32>,
    cleanups: Vec<u32>,
    updated_at: Option<u64>,
    disposed: bool,
}

pub struct Graph {
    nodes: Vec<Node>,
    listener: u32,
    owner: u32,
    updates: Option<Vec<u32>>,
    effects: Option<Vec<u32>>,
    exec: u64,
    runs: u64,
    pub error: Option<String>,
}

fn is_node(id: u32) -> bool {
    id < DISPOSED_OWNER
}

fn is_computation(kind: Kind) -> bool {
    matches!(kind, Kind::Memo | Kind::Effect | Kind::UserEffect)
}

impl Graph {
    pub fn new() -> Self {
        Self {
            nodes: vec![Node {
                kind: Kind::Signal,
                state: CLEAN,
                sources: Vec::new(),
                observers: Vec::new(),
                owner: NONE,
                owned: Vec::new(),
                cleanups: Vec::new(),
                updated_at: None,
                disposed: true,
            }],
            listener: NONE,
            owner: NONE,
            updates: None,
            effects: None,
            exec: 0,
            runs: 0,
            error: None,
        }
    }

    pub fn alloc_signal(&mut self) -> u32 {
        self.alloc(Kind::Signal)
    }

    pub fn alloc_computation(&mut self, kind: Kind) -> Result<u32, String> {
        if !is_computation(kind) {
            return Err("not a computation".into());
        }
        Ok(self.alloc(kind))
    }

    fn alloc(&mut self, kind: Kind) -> u32 {
        let owner = self.owner;
        let id = self.nodes.len() as u32;
        self.nodes.push(Node {
            kind,
            state: if kind == Kind::Signal { CLEAN } else { STALE },
            sources: Vec::new(),
            observers: Vec::new(),
            owner: if is_node(owner) { owner } else { NONE },
            owned: Vec::new(),
            cleanups: Vec::new(),
            updated_at: None,
            disposed: false,
        });
        if is_node(owner) && kind != Kind::Signal {
            self.nodes[owner as usize].owned.push(id);
        }
        id
    }
}

fn kind_of(g: *mut Graph, id: u32) -> Result<Kind, String> {
    match unsafe { (*g).nodes.get(id as usize) } {
        Some(n) if !(n.disposed && n.kind == Kind::Signal) => Ok(n.kind),
        _ => Err(format!("unknown signal {id}")),
    }
}

pub fn mount(g: *mut Graph, id: u32, host: &mut dyn Sched) -> Result<(), String> {
    let kind = kind_of(g, id)?;
    if !is_computation(kind) {
        return Err(format!("signal {id} is not a computation"));
    }
    if unsafe { (*g).owner == DISPOSED_OWNER || (*g).nodes[id as usize].disposed } {
        dispose(g, id, host);
        return Ok(());
    }
    if kind == Kind::UserEffect && unsafe { (*g).effects.is_some() } {
        unsafe { (*g).queue_effect(id) };
    } else {
        update(g, id, host);
    }
    Ok(())
}

pub fn read(g: *mut Graph, id: u32, host: &mut dyn Sched) -> Result<(), String> {
    let n = unsafe { (*g).nodes.get(id as usize) }.ok_or_else(|| format!("unknown signal {id}"))?;
    if n.disposed && n.kind != Kind::Memo {
        return Err(format!("unknown signal {id}"));
    }
    if n.kind == Kind::Memo && !n.disposed && n.state != CLEAN {
        if n.state == STALE {
            update(g, id, host);
        } else {
            side(g, host, Some((id, NONE)), None);
        }
    }
    unsafe {
        if (*g).nodes.get(id as usize).is_some_and(|n| !n.disposed) {
            (*g).track(id);
        }
    }
    Ok(())
}

pub fn notify(g: *mut Graph, id: u32, host: &mut dyn Sched) -> Result<(), String> {
    let n = unsafe { (*g).nodes.get(id as usize) }.ok_or_else(|| format!("unknown signal {id}"))?;
    if n.kind != Kind::Signal || n.disposed {
        return Err(format!("unknown signal {id}"));
    }
    side(g, host, None, Some(id));
    Ok(())
}

pub fn on_cleanup(g: *mut Graph, id: u32) -> bool {
    let owner = unsafe { (*g).owner };
    if !is_node(owner) {
        return false;
    }
    unsafe { (*g).nodes[owner as usize].cleanups.push(id) };
    true
}

pub fn dispose(g: *mut Graph, id: u32, host: &mut dyn Sched) {
    if !is_node(id) || unsafe { (*g).nodes.get(id as usize).is_none_or(|n| n.disposed) } {
        return;
    }
    unsafe {
        (*g).nodes[id as usize].disposed = true;
        if (*g).owner == id {
            (*g).owner = DISPOSED_OWNER;
        }
        if (*g).listener == id {
            (*g).listener = NONE;
        }
    }
    clean(g, id, host);
    unsafe {
        (*g).unlink_observers(id);
        (*g).nodes[id as usize].state = CLEAN;
    }
}

pub fn run_scheduled(g: *mut Graph, init: bool, host: &mut dyn Sched) {
    let Some(wait) = begin(g, init) else {
        host.body();
        return;
    };
    host.body();
    complete(g, wait, host);
}

fn begin(g: *mut Graph, init: bool) -> Option<bool> {
    unsafe {
        if (*g).updates.is_some() {
            return None;
        }
        if !init {
            (*g).updates = Some(Vec::new());
        }
        let wait = if (*g).effects.is_some() {
            true
        } else {
            (*g).effects = Some(Vec::new());
            false
        };
        (*g).exec += 1;
        Some(wait)
    }
}

fn complete(g: *mut Graph, wait: bool, host: &mut dyn Sched) {
    run_queued(g, host);
    if wait {
        return;
    }
    loop {
        let effects = unsafe { (*g).effects.take().unwrap_or_default() };
        if effects.is_empty() {
            break;
        }
        unsafe {
            (*g).updates = Some(Vec::new());
            (*g).effects = Some(Vec::new());
            (*g).exec += 1;
        }
        run_effects(g, &effects, host);
        run_queued(g, host);
    }
    unsafe { (*g).runs = 0 };
}

fn run_queued(g: *mut Graph, host: &mut dyn Sched) {
    let mut i = 0;
    while let Some(next) = unsafe { (*g).updates.as_ref().and_then(|q| q.get(i).copied()) } {
        i += 1;
        if !count_run(g) {
            break;
        }
        run_top(g, next, host);
    }
    unsafe { (*g).updates = None };
}

fn count_run(g: *mut Graph) -> bool {
    unsafe {
        (*g).runs += 1;
        if (*g).runs <= MAX_RUNS {
            return true;
        }
        (*g).updates = Some(Vec::new());
        (*g).effects = None;
        (*g).error = Some("reactive graph ran away".into());
        false
    }
}

fn run_effects(g: *mut Graph, effects: &[u32], host: &mut dyn Sched) {
    for user in [false, true] {
        for &id in effects {
            let kind = unsafe { (*g).nodes[id as usize].kind };
            if (kind == Kind::UserEffect) == user && count_run(g) {
                run_top(g, id, host);
            }
        }
    }
}

fn run_top(g: *mut Graph, id: u32, host: &mut dyn Sched) {
    let (state, disposed, owner, exec, kind) = unsafe {
        let n = &(*g).nodes[id as usize];
        (n.state, n.disposed, n.owner, (*g).exec, n.kind)
    };
    if state == CLEAN || disposed || !is_computation(kind) {
        return;
    }
    if state == PENDING {
        side(g, host, Some((id, NONE)), None);
        return;
    }
    let mut ancestors = vec![id];
    let mut at = owner;
    while is_node(at) {
        let (updated, st, parent_kind, parent) = unsafe {
            let n = &(*g).nodes[at as usize];
            (n.updated_at, n.state, n.kind, n.owner)
        };
        if updated.is_some_and(|t| t >= exec) {
            break;
        }
        if st != CLEAN && is_computation(parent_kind) {
            ancestors.push(at);
        }
        at = parent;
    }
    for &node_id in ancestors.iter().rev() {
        let (dead, state) = unsafe {
            let n = &(*g).nodes[node_id as usize];
            (n.disposed, n.state)
        };
        if dead {
            continue;
        }
        if state == STALE {
            update(g, node_id, host);
        } else if state == PENDING {
            side(g, host, Some((node_id, ancestors[0])), None);
        }
    }
}

fn look_upstream(g: *mut Graph, id: u32, ignore: u32, host: &mut dyn Sched) {
    unsafe { (*g).nodes[id as usize].state = CLEAN };
    let mut i = 0;
    loop {
        let source = unsafe { (*g).nodes[id as usize].sources.get(i).map(|s| s.0) };
        let Some(source) = source else { break };
        i += 1;
        let (kind, state, updated) = unsafe {
            let s = &(*g).nodes[source as usize];
            (s.kind, s.state, s.updated_at)
        };
        if kind != Kind::Memo {
            continue;
        }
        if state == STALE {
            let exec = unsafe { (*g).exec };
            if source != ignore && updated.is_none_or(|t| t < exec) {
                run_top(g, source, host);
            }
        } else if state == PENDING {
            look_upstream(g, source, ignore, host);
        }
    }
}

fn update(g: *mut Graph, id: u32, host: &mut dyn Sched) {
    if unsafe {
        let n = &(*g).nodes[id as usize];
        n.disposed || !is_computation(n.kind)
    } {
        return;
    }
    clean(g, id, host);
    let time = unsafe { (*g).exec };
    let had = unsafe { (*g).nodes[id as usize].updated_at.is_some() };
    let kind = unsafe { (*g).nodes[id as usize].kind };
    let prev_listener = unsafe { (*g).listener };
    let prev_owner = unsafe { (*g).owner };
    unsafe {
        (*g).listener = id;
        (*g).owner = id;
    }
    let result = host.run(id);
    unsafe {
        (*g).listener = prev_listener;
        if (*g).owner == id {
            (*g).owner = if is_node(prev_owner)
                && (*g)
                    .nodes
                    .get(prev_owner as usize)
                    .is_some_and(|n| !n.disposed)
            {
                prev_owner
            } else if is_node(prev_owner) {
                DISPOSED_OWNER
            } else {
                prev_owner
            };
        }
    }
    if result < 0 {
        unsafe {
            (*g).error = Some(format!("computation {id} failed"));
            let n = &mut (*g).nodes[id as usize];
            if n.kind == Kind::Memo {
                n.state = STALE;
            }
            n.updated_at = Some(time.saturating_add(1));
        }
        if kind == Kind::Memo {
            let owned = unsafe { std::mem::take(&mut (*g).nodes[id as usize].owned) };
            for child in owned {
                dispose(g, child, host);
            }
        }
        return;
    }
    if unsafe { (*g).nodes[id as usize].disposed } {
        return;
    }
    if unsafe { (*g).nodes[id as usize].updated_at.is_some_and(|t| t > time) } {
        return;
    }
    unsafe { (*g).nodes[id as usize].updated_at = Some(time) };
    if had && kind == Kind::Memo && result == 1 {
        side(g, host, None, Some(id));
    }
}

fn side(g: *mut Graph, host: &mut dyn Sched, look: Option<(u32, u32)>, mark: Option<u32>) {
    let saved = unsafe { (*g).updates.take() };
    let Some(wait) = begin(g, false) else {
        apply_side(g, host, look, mark);
        restore_updates(g, saved);
        return;
    };
    apply_side(g, host, look, mark);
    complete(g, wait, host);
    restore_updates(g, saved);
}

fn apply_side(g: *mut Graph, host: &mut dyn Sched, look: Option<(u32, u32)>, mark: Option<u32>) {
    if let Some((id, ignore)) = look {
        look_upstream(g, id, ignore, host);
    }
    if let Some(id) = mark {
        mark_observers(g, id);
    }
}

fn restore_updates(g: *mut Graph, saved: Option<Vec<u32>>) {
    unsafe {
        if (*g).updates.is_none() {
            (*g).updates = saved;
        }
    }
}

fn mark_observers(g: *mut Graph, id: u32) {
    if unsafe { (*g).nodes.get(id as usize).is_none() } {
        return;
    }
    let mut i = 0;
    while i < unsafe { (*g).nodes[id as usize].observers.len() } {
        let o = unsafe { (*g).nodes[id as usize].observers[i].0 };
        i += 1;
        let (disposed, state, kind, has_obs) = unsafe {
            let n = &(*g).nodes[o as usize];
            (n.disposed, n.state, n.kind, !n.observers.is_empty())
        };
        if disposed {
            continue;
        }
        if state == CLEAN {
            if kind == Kind::Memo {
                unsafe { (*g).queue_update(o) };
            } else if is_computation(kind) {
                unsafe { (*g).queue_effect(o) };
            }
            if has_obs {
                mark_downstream(g, o);
            }
        }
        unsafe { (*g).nodes[o as usize].state = STALE };
    }
}

fn mark_downstream(g: *mut Graph, id: u32) {
    let mut i = 0;
    while i < unsafe { (*g).nodes[id as usize].observers.len() } {
        let o = unsafe { (*g).nodes[id as usize].observers[i].0 };
        i += 1;
        let (state, disposed, kind, has_obs) = unsafe {
            let n = &(*g).nodes[o as usize];
            (n.state, n.disposed, n.kind, !n.observers.is_empty())
        };
        if state == CLEAN && !disposed {
            unsafe { (*g).nodes[o as usize].state = PENDING };
            if kind == Kind::Memo {
                unsafe { (*g).queue_update(o) };
            } else if is_computation(kind) {
                unsafe { (*g).queue_effect(o) };
            }
            if has_obs {
                mark_downstream(g, o);
            }
        }
    }
}

fn clean(g: *mut Graph, id: u32, host: &mut dyn Sched) {
    unsafe { (*g).unlink_sources(id) };
    while let Some(child) = unsafe { (*g).nodes[id as usize].owned.pop() } {
        dispose(g, child, host);
    }
    while let Some(c) = unsafe { (*g).nodes[id as usize].cleanups.pop() } {
        host.cleanup(c);
    }
    unsafe { (*g).nodes[id as usize].state = CLEAN };
}

impl Graph {
    fn queue_effect(&mut self, id: u32) {
        match &mut self.effects {
            Some(q) => q.push(id),
            None => self.effects = Some(vec![id]),
        }
    }

    fn queue_update(&mut self, id: u32) {
        match &mut self.updates {
            Some(q) => q.push(id),
            None => self.updates = Some(vec![id]),
        }
    }

    fn track(&mut self, id: u32) {
        let listener = self.listener;
        if !is_node(listener) {
            return;
        }
        let sources_end = self.nodes[listener as usize].sources.last().map(|s| s.0);
        if self.nodes[listener as usize].disposed
            || !is_computation(self.nodes[listener as usize].kind)
        {
            return;
        }
        if sources_end == Some(id) {
            return;
        }
        let slot = self.nodes[id as usize].observers.len() as u32;
        let back = self.nodes[listener as usize].sources.len() as u32;
        self.nodes[listener as usize].sources.push((id, slot));
        self.nodes[id as usize].observers.push((listener, back));
    }

    fn unlink_sources(&mut self, id: u32) {
        while let Some((src, slot)) = self.nodes[id as usize].sources.pop() {
            let obs = &mut self.nodes[src as usize].observers;
            let Some(last) = obs.pop() else { continue };
            if (slot as usize) < obs.len() {
                obs[slot as usize] = last;
                self.nodes[last.0 as usize].sources[last.1 as usize].1 = slot;
            }
        }
    }

    fn unlink_observers(&mut self, id: u32) {
        while let Some((o, slot)) = self.nodes[id as usize].observers.pop() {
            let srcs = &mut self.nodes[o as usize].sources;
            let Some(last) = srcs.pop() else { continue };
            if (slot as usize) < srcs.len() {
                srcs[slot as usize] = last;
                self.nodes[last.0 as usize].observers[last.1 as usize].1 = slot;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Host<'a> {
        graph: *mut Graph,
        runs: &'a RefCell<Vec<u32>>,
        memo: u32,
        signal: u32,
        value: &'a RefCell<i32>,
    }

    impl Sched for Host<'_> {
        fn run(&mut self, id: u32) -> i32 {
            self.runs.borrow_mut().push(id);
            let kind = unsafe { (*self.graph).nodes[id as usize].kind };
            if kind == Kind::Memo {
                let _ = read(self.graph, self.signal, self);
                let next = *self.value.borrow();
                let seen = unsafe { (*self.graph).nodes[id as usize].updated_at.is_some() };
                return i32::from(seen && next != 0);
            }
            let _ = read(self.graph, self.memo, self);
            0
        }
        fn cleanup(&mut self, _: u32) {}
    }

    #[test]
    fn memo_runs_once_and_before_its_readers() {
        let mut graph = Graph::new();
        let g = &mut graph as *mut Graph;
        let runs = RefCell::new(Vec::new());
        let value = RefCell::new(0);
        let signal = graph.alloc_signal();
        let memo = graph.alloc_computation(Kind::Memo).unwrap();
        let mut host = Host {
            graph: g,
            runs: &runs,
            memo,
            signal,
            value: &value,
        };
        mount(g, memo, &mut host).unwrap();
        let effect = unsafe { (*g).alloc_computation(Kind::Effect).unwrap() };
        mount(g, effect, &mut host).unwrap();
        *value.borrow_mut() = 1;
        notify(g, signal, &mut host).unwrap();
        let log = runs.borrow().clone();
        assert_eq!(log.iter().filter(|id| **id == memo).count(), 2);
        assert_eq!(log.iter().filter(|id| **id == effect).count(), 2);
        let memo_at = log.iter().rposition(|id| *id == memo).unwrap();
        let effect_at = log.iter().rposition(|id| *id == effect).unwrap();
        assert!(memo_at < effect_at);
    }

    #[test]
    fn rerun_fires_cleanups_before_the_body() {
        let mut graph = Graph::new();
        let g = &mut graph as *mut Graph;
        let runs = RefCell::new(0);
        let cleanups = RefCell::new(Vec::new());
        struct H<'a> {
            graph: *mut Graph,
            runs: &'a RefCell<u32>,
            cleanups: &'a RefCell<Vec<u32>>,
        }
        impl Sched for H<'_> {
            fn run(&mut self, _: u32) -> i32 {
                *self.runs.borrow_mut() += 1;
                assert!(on_cleanup(self.graph, 7));
                let _ = read(self.graph, 1, self);
                0
            }
            fn cleanup(&mut self, id: u32) {
                assert_eq!(*self.runs.borrow(), 1);
                self.cleanups.borrow_mut().push(id);
            }
        }
        let _signal = graph.alloc_signal();
        let effect = graph.alloc_computation(Kind::Effect).unwrap();
        let mut host = H {
            graph: g,
            runs: &runs,
            cleanups: &cleanups,
        };
        mount(g, effect, &mut host).unwrap();
        notify(g, 1, &mut host).unwrap();
        assert_eq!(cleanups.borrow().as_slice(), &[7]);
        assert_eq!(*runs.borrow(), 2);
    }

    #[test]
    fn user_effect_runs_after_render_effects() {
        let mut graph = Graph::new();
        let g = &mut graph as *mut Graph;
        let log = RefCell::new(Vec::new());
        struct H<'a> {
            graph: *mut Graph,
            log: &'a RefCell<Vec<u32>>,
            render: u32,
            user: u32,
        }
        impl Sched for H<'_> {
            fn body(&mut self) {
                self.render = unsafe { (*self.graph).alloc_computation(Kind::Effect).unwrap() };
                self.user = unsafe { (*self.graph).alloc_computation(Kind::UserEffect).unwrap() };
                mount(self.graph, self.render, self).unwrap();
                mount(self.graph, self.user, self).unwrap();
            }
            fn run(&mut self, id: u32) -> i32 {
                self.log.borrow_mut().push(id);
                0
            }
            fn cleanup(&mut self, _: u32) {}
        }
        let mut host = H {
            graph: g,
            log: &log,
            render: 0,
            user: 0,
        };
        run_scheduled(g, true, &mut host);
        assert_eq!(log.borrow().as_slice(), &[host.render, host.user]);
    }

    #[test]
    fn stale_source_memo_updates_before_its_reader() {
        let mut graph = Graph::new();
        let g = &mut graph as *mut Graph;
        let log = RefCell::new(Vec::new());
        let signal = graph.alloc_signal();
        struct H<'a> {
            graph: *mut Graph,
            log: &'a RefCell<Vec<u32>>,
            inner: u32,
        }
        impl Sched for H<'_> {
            fn run(&mut self, id: u32) -> i32 {
                self.log.borrow_mut().push(id);
                if id == self.inner {
                    let _ = read(self.graph, 1, self);
                } else {
                    let _ = read(self.graph, self.inner, self);
                }
                1
            }
            fn cleanup(&mut self, _: u32) {}
        }
        let inner = graph.alloc_computation(Kind::Memo).unwrap();
        let mut host = H {
            graph: g,
            log: &log,
            inner,
        };
        mount(g, inner, &mut host).unwrap();
        let outer = unsafe { (*g).alloc_computation(Kind::Memo).unwrap() };
        mount(g, outer, &mut host).unwrap();
        log.borrow_mut().clear();
        notify(g, signal, &mut host).unwrap();
        assert_eq!(log.borrow().as_slice(), &[inner, outer]);
    }
}
