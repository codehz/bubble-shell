//! Scrollbar thumbs: visibility, geometry, and drag-to-scroll.

use bubble_core::{
    encode, spawn_ops, Kind, NodeDesc, Op, PaintCmd, PointerAction, PointerInput, Shell, Value,
};

fn layout(shell: &mut Shell) {
    shell.layout(&mut |_, _, _, _, _| (0.0, 0.0)).unwrap();
}

fn prop(scrollbar: u32, key: &str, value: Value) -> Op {
    Op::SetScrollBarProp {
        scrollbar,
        key: key.into(),
        value,
    }
}

fn blue_thumbs(shell: &Shell) -> Vec<(f32, f32, f32, f32)> {
    shell
        .paint()
        .iter()
        .filter_map(|cmd| match cmd {
            PaintCmd::Fill {
                x, y, w, h, color, ..
            } if color.to_argb() == 0xff00_00ff => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .collect()
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.05
}

fn pointer(shell: &mut Shell, action: PointerAction, x: f32, y: f32, time_ms: f64) {
    shell.pointer(PointerInput {
        action,
        id: 1,
        x,
        y,
        time_ms,
        buttons: if action == PointerAction::Up { 0 } else { 1 },
        pointer_kind: 0,
        path: Vec::new(),
    });
}

/// Vertical scroller, 200px viewport, 600px of content, one blue scrollbar.
fn vertical_shell(auto_hide: bool) -> (Shell, u32) {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(240.0, 400.0);
    let mut root = NodeDesc::view(2, 1);
    root.scroll_y = true;
    root.width = Some(160.0);
    root.height = Some(200.0);
    root.flex_shrink = 0.0;
    let mut ops = spawn_ops(root);
    for (id, parent) in [(3, 2), (4, 2), (5, 2)] {
        let mut child = NodeDesc::view(id, parent);
        child.width = Some(160.0);
        child.height = Some(200.0);
        child.flex_shrink = 0.0;
        ops.extend(spawn_ops(child));
    }
    ops.push(Op::SpawnScrollBar {
        scrollbar: 6,
        target: 2,
        index: 0,
    });
    ops.push(prop(6, "scrollbarWidth", Value::Float(6.0)));
    ops.push(prop(6, "scrollbarColor", Value::Int(0xff00_00ff)));
    ops.push(prop(6, "autoHide", Value::Bool(auto_hide)));
    shell.apply_ops(&encode(1, &ops)).unwrap();
    layout(&mut shell);
    (shell, 2)
}

fn thumb_length(track: f32, viewport: f32, max_scroll: f32, min_length: f32) -> f32 {
    let content = max_scroll + viewport;
    let ratio = (viewport / content).clamp(0.0, 1.0);
    (track * ratio).max(min_length).min(track)
}

#[test]
fn vertical_thumb_tracks_scroll() {
    let (mut shell, id) = vertical_shell(false);
    let (x, y, w, h) = shell.node_box(id).unwrap();
    let thumbs = blue_thumbs(&shell);
    assert_eq!(thumbs.len(), 1, "one vertical thumb, got {thumbs:?}");
    let (tx, ty, tw, th) = thumbs[0];
    let margin = 2.0;
    let thickness = 6.0;
    let track = h - 2.0 * margin;
    shell
        .apply_ops(&encode(
            2,
            &[Op::ScrollTo {
                handle: id,
                x: 0.0,
                y: 1.0e6,
            }],
        ))
        .unwrap();
    let max = shell.scroll_of(id).unwrap().1;
    assert!(near(max, 400.0), "content 600 in a 200 viewport, max {max}");
    let length = thumb_length(track, h, max, 20.0);
    assert!(near(tw, thickness), "thickness {tw}");
    assert!(near(th, length), "thumb length {th} expected {length}");
    assert!(near(tx, x + w - margin - thickness), "right edge {tx}");
    assert!(near(ty, y + margin), "top of track {ty}");

    shell
        .apply_ops(&encode(
            3,
            &[Op::ScrollTo {
                handle: id,
                x: 0.0,
                y: 200.0,
            }],
        ))
        .unwrap();
    let (_, ty, _, th) = blue_thumbs(&shell)[0];
    let offset = (200.0 / max) * (track - length);
    assert!(near(th, length), "length stable {th}");
    assert!(
        near(ty, y + margin + offset),
        "scrolled thumb y {ty} expected {}",
        y + margin + offset
    );
}

#[test]
fn auto_hide_follows_scroll_clock() {
    let (mut shell, id) = vertical_shell(true);
    assert_eq!(blue_thumbs(&shell).len(), 1, "visible at t=0");
    shell.advance_gestures(1001.0);
    assert!(
        blue_thumbs(&shell).is_empty(),
        "hidden 1000ms after the epoch"
    );
    assert!(shell.scroll_path(&[id], 0.0, 40.0).changed());
    assert_eq!(blue_thumbs(&shell).len(), 1, "scroll shows the thumb again");
    shell.advance_gestures(1001.0 + 1001.0);
    assert!(
        blue_thumbs(&shell).is_empty(),
        "hides again after another 1000ms"
    );

    shell
        .apply_ops(&encode(4, &[prop(6, "autoHide", Value::Bool(false))]))
        .unwrap();
    assert_eq!(blue_thumbs(&shell).len(), 1);
    shell.advance_gestures(10_000.0);
    assert_eq!(blue_thumbs(&shell).len(), 1, "autoHide false stays painted");
}

#[test]
fn drag_thumb_scrolls_and_releases() {
    let (mut shell, id) = vertical_shell(false);
    let (_, y, _, h) = shell.node_box(id).unwrap();
    let (tx, ty, tw, th) = blue_thumbs(&shell)[0];
    shell
        .apply_ops(&encode(
            2,
            &[Op::ScrollTo {
                handle: id,
                x: 1.0e6,
                y: 1.0e6,
            }],
        ))
        .unwrap();
    let max = shell.scroll_of(id).unwrap().1;
    shell
        .apply_ops(&encode(
            3,
            &[Op::ScrollTo {
                handle: id,
                x: 0.0,
                y: 0.0,
            }],
        ))
        .unwrap();
    let margin = 2.0;
    let track = h - 2.0 * margin;
    let length = thumb_length(track, h, max, 20.0);
    let scrollable = track - length;
    pointer(
        &mut shell,
        PointerAction::Down,
        tx + tw / 2.0,
        ty + th / 2.0,
        10.0,
    );
    pointer(
        &mut shell,
        PointerAction::Move,
        tx + tw / 2.0,
        ty + th / 2.0 + 50.0,
        20.0,
    );
    let top = shell.scroll_of(id).unwrap().1;
    let expected = 50.0 * max / scrollable;
    assert!(
        near(top, expected),
        "scroll_top {top} expected {expected} (thumb y {ty}, box y {y})"
    );
    pointer(
        &mut shell,
        PointerAction::Up,
        tx + tw / 2.0,
        ty + th / 2.0 + 50.0,
        30.0,
    );
    pointer(
        &mut shell,
        PointerAction::Move,
        tx + tw / 2.0,
        ty + th / 2.0 + 90.0,
        40.0,
    );
    assert!(
        near(shell.scroll_of(id).unwrap().1, top),
        "move after up does not keep dragging"
    );
}

#[test]
fn horizontal_thumb_sits_on_the_bottom_edge() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(240.0, 200.0);
    let mut root = NodeDesc::view(2, 1);
    root.scroll_x = true;
    root.width = Some(200.0);
    root.height = Some(80.0);
    root.flex_shrink = 0.0;
    let mut child = NodeDesc::view(3, 2);
    child.width = Some(600.0);
    child.height = Some(40.0);
    child.flex_shrink = 0.0;
    let mut ops = spawn_ops(root);
    ops.extend(spawn_ops(child));
    ops.push(Op::SpawnScrollBar {
        scrollbar: 4,
        target: 2,
        index: 0,
    });
    ops.push(prop(4, "axis", Value::String("horizontal".into())));
    ops.push(prop(4, "scrollbarWidth", Value::Float(6.0)));
    ops.push(prop(4, "scrollbarColor", Value::Int(0xff00_00ff)));
    ops.push(prop(4, "autoHide", Value::Bool(false)));
    shell.apply_ops(&encode(1, &ops)).unwrap();
    layout(&mut shell);
    let (x, y, w, h) = shell.node_box(2).unwrap();
    let (tx, ty, tw, th) = blue_thumbs(&shell)[0];
    shell
        .apply_ops(&encode(
            2,
            &[Op::ScrollTo {
                handle: 2,
                x: 1.0e6,
                y: 0.0,
            }],
        ))
        .unwrap();
    let max = shell.scroll_of(2).unwrap().0;
    assert!(near(max, 400.0), "horizontal max {max}");
    let margin = 2.0;
    let track = w - 2.0 * margin;
    let length = thumb_length(track, w, max, 20.0);
    assert!(near(th, 6.0), "horizontal thickness {th}");
    assert!(near(tw, length), "horizontal length {tw}");
    assert!(near(tx, x + margin), "left of track {tx}");
    assert!(near(ty, y + h - margin - 6.0), "bottom edge {ty}");

    shell
        .apply_ops(&encode(
            3,
            &[Op::ScrollTo {
                handle: 2,
                x: 0.0,
                y: 0.0,
            }],
        ))
        .unwrap();
    let (tx, ty, tw, th) = blue_thumbs(&shell)[0];
    pointer(
        &mut shell,
        PointerAction::Down,
        tx + tw / 2.0,
        ty + th / 2.0,
        10.0,
    );
    pointer(
        &mut shell,
        PointerAction::Move,
        tx + tw / 2.0 + 50.0,
        ty + th / 2.0,
        20.0,
    );
    let left = shell.scroll_of(2).unwrap().0;
    let expected = 50.0 * max / (track - length);
    assert!(
        near(left, expected),
        "scroll_left {left} expected {expected}"
    );
}

#[test]
fn no_thumb_when_content_fits() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut root = NodeDesc::view(2, 1);
    root.scroll_y = true;
    root.width = Some(100.0);
    root.height = Some(200.0);
    root.flex_shrink = 0.0;
    let mut child = NodeDesc::view(3, 2);
    child.width = Some(100.0);
    child.height = Some(80.0);
    child.flex_shrink = 0.0;
    let mut ops = spawn_ops(root);
    ops.extend(spawn_ops(child));
    ops.push(Op::SpawnScrollBar {
        scrollbar: 4,
        target: 2,
        index: 0,
    });
    ops.push(prop(4, "scrollbarColor", Value::Int(0xff00_00ff)));
    ops.push(prop(4, "autoHide", Value::Bool(false)));
    shell.apply_ops(&encode(1, &ops)).unwrap();
    layout(&mut shell);
    assert!(blue_thumbs(&shell).is_empty(), "no overflow, no thumb");
}

#[test]
fn both_axes_track_independently() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(300.0, 300.0);
    let mut root = NodeDesc::view(2, 1);
    root.scroll_x = true;
    root.scroll_y = true;
    root.width = Some(200.0);
    root.height = Some(200.0);
    root.flex_shrink = 0.0;
    let mut child = NodeDesc::view(3, 2);
    child.width = Some(600.0);
    child.height = Some(600.0);
    child.flex_shrink = 0.0;
    let mut ops = spawn_ops(root);
    ops.extend(spawn_ops(child));
    ops.push(Op::SpawnScrollBar {
        scrollbar: 4,
        target: 2,
        index: 0,
    });
    ops.push(Op::SpawnScrollBar {
        scrollbar: 5,
        target: 2,
        index: 1,
    });
    ops.push(prop(4, "scrollbarWidth", Value::Float(6.0)));
    ops.push(prop(4, "scrollbarColor", Value::Int(0xff00_00ff)));
    ops.push(prop(4, "autoHide", Value::Bool(false)));
    ops.push(prop(5, "axis", Value::String("horizontal".into())));
    ops.push(prop(5, "scrollbarWidth", Value::Float(6.0)));
    ops.push(prop(5, "scrollbarColor", Value::Int(0xff00_00ff)));
    ops.push(prop(5, "autoHide", Value::Bool(false)));
    shell.apply_ops(&encode(1, &ops)).unwrap();
    layout(&mut shell);
    let thumbs = blue_thumbs(&shell);
    assert_eq!(thumbs.len(), 2, "both axes, got {thumbs:?}");
    let vertical_thumb = thumbs.iter().find(|(_, _, w, h)| *h > *w).unwrap();
    let horizontal_thumb = thumbs.iter().find(|(_, _, w, h)| *w > *h).unwrap();
    assert!(near(vertical_thumb.2, 6.0));
    assert!(near(horizontal_thumb.3, 6.0));

    shell
        .apply_ops(&encode(
            2,
            &[Op::ScrollTo {
                handle: 2,
                x: 0.0,
                y: 200.0,
            }],
        ))
        .unwrap();
    let next = blue_thumbs(&shell);
    let vertical_next = next.iter().find(|(_, _, w, h)| *h > *w).unwrap();
    let horizontal_next = next.iter().find(|(_, _, w, h)| *w > *h).unwrap();
    assert!(
        vertical_next.1 > vertical_thumb.1,
        "vertical thumb moved down"
    );
    assert!(
        near(horizontal_next.0, horizontal_thumb.0),
        "horizontal thumb stays put when only y scrolls"
    );
}

#[test]
fn scrollbar_kind_spawn_rejected() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(100.0, 100.0);
    let mut bar = NodeDesc::view(2, 1);
    bar.kind = Kind::ScrollBar;
    let err = shell.spawn(bar).unwrap_err();
    assert!(err.contains("SpawnScrollBar op"), "got: {err}");
}
