use bubble_core::{
    encode, A11yAction, NodeDesc, Op, PaintCmd, PointerAction, PointerInput, Shell, Value,
};

fn layout(shell: &mut Shell) {
    shell
        .layout(&mut |text, font, _, _, _| (text.len() as f32 * font * 0.5, font))
        .unwrap();
}

fn set(shell: &mut Shell, handle: u32, key: &str, value: &str) {
    shell
        .apply_ops(&encode(
            1,
            &[Op::SetProp {
                handle,
                key: key.into(),
                value: Value::String(value.into()),
            }],
        ))
        .unwrap();
}

fn pointer(shell: &mut Shell, action: PointerAction, x: f32, y: f32, time_ms: f64) {
    shell.pointer(PointerInput {
        action,
        id: 1,
        x,
        y,
        time_ms,
        buttons: u32::from(action != PointerAction::Up),
        pointer_kind: 0,
        path: Vec::new(),
    });
}

#[test]
fn overflow_clips_each_axis_at_the_padding_box_and_resets_scroll() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut parent = NodeDesc::view(2, 1);
    parent.width = Some(60.0);
    parent.height = Some(60.0);
    parent.flex_shrink = 0.0;
    parent.padding = [10.0; 4];
    let mut child = NodeDesc::view(3, 2);
    child.width = Some(120.0);
    child.height = Some(120.0);
    child.flex_shrink = 0.0;
    child.handler = 77;
    child.background = 0xff00_ff00;
    shell
        .apply_ops(&encode(
            1,
            &[Op::Spawn { desc: parent }, Op::Spawn { desc: child }],
        ))
        .unwrap();
    set(&mut shell, 3, "position", "absolute");
    set(&mut shell, 2, "overflowX", "hidden");
    layout(&mut shell);
    assert_eq!(shell.tap(55.0, 20.0), Some(77), "padding remains visible");
    assert_eq!(shell.tap(70.0, 20.0), None, "x overflow is clipped");
    assert_eq!(
        shell.tap(20.0, 80.0),
        Some(77),
        "visible y overflows parent"
    );
    assert!(shell.paint().iter().any(|cmd| matches!(cmd,
        PaintCmd::PushClip { x, y, w, h }
        if (*x, *y, *w, *h) == (0.0, 0.0, 60.0, 200.0)
    )));
    assert!(!shell.scroll_wheel(20.0, 20.0, 30.0, 30.0));
    set(&mut shell, 2, "overflowY", "scroll");
    layout(&mut shell);
    assert_eq!(shell.tap(20.0, 80.0), None);
    assert!(shell.scroll_wheel(20.0, 20.0, 0.0, 30.0));
    assert_eq!(shell.scroll_of(2).unwrap(), (0.0, 30.0));
    set(&mut shell, 2, "overflowY", "hidden");
    layout(&mut shell);
    assert_eq!(shell.scroll_of(2).unwrap(), (0.0, 0.0));
    assert!(!shell.scroll_wheel(20.0, 20.0, 0.0, 30.0));
    set(&mut shell, 2, "overflow", "visible");
    layout(&mut shell);
    assert_eq!(shell.tap(70.0, 80.0), Some(77));
}

#[test]
fn subtree_hiding_preserves_state_but_updates_layout_focus_and_semantics() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut parent = NodeDesc::view(2, 1);
    parent.height = Some(60.0);
    parent.flex_shrink = 0.0;
    let mut child = NodeDesc::text(3, 2, "retained");
    child.handler = 77;
    let mut sibling = NodeDesc::view(4, 1);
    sibling.height = Some(20.0);
    shell
        .apply_ops(&encode(
            1,
            &[
                Op::Spawn { desc: parent },
                Op::Spawn { desc: child },
                Op::Spawn { desc: sibling },
            ],
        ))
        .unwrap();
    set(&mut shell, 3, "role", "button");
    set(&mut shell, 3, "visibility", "visible");
    layout(&mut shell);
    shell.focus(3).unwrap();
    assert_eq!(shell.node_box(4).unwrap().1, 60.0);
    set(&mut shell, 2, "visibility", "hidden");
    layout(&mut shell);
    assert_eq!(shell.focused(), 0);
    assert!(shell.focus(3).is_err());
    assert!(!shell.focus_next(true));
    assert_eq!(shell.node_box(4).unwrap().1, 60.0);
    assert_eq!(shell.tap(1.0, 1.0), None);
    assert!(!shell
        .paint()
        .iter()
        .any(|cmd| matches!(cmd, PaintCmd::Text { text, .. } if text == "retained")));
    let snapshot = shell.semantics();
    if let Some(node) = snapshot.nodes.iter().find(|node| node.handle == 3) {
        assert!(node.hidden);
        assert_eq!(node.actions, 0);
    }
    assert!(shell.a11y_action(3, A11yAction::Focus).is_err());
    set(&mut shell, 2, "visibility", "visible");
    layout(&mut shell);
    assert_eq!(shell.tap(1.0, 1.0), Some(77));
    assert_eq!(shell.text(3).unwrap(), "retained");
    shell.focus(3).unwrap();
    set(&mut shell, 2, "display", "none");
    layout(&mut shell);
    assert_eq!(shell.focused(), 0);
    assert_eq!(shell.node_box(4).unwrap().1, 0.0);
    assert!(shell.focus(3).is_err());
    assert_eq!(shell.tap(1.0, 1.0), None);
    set(&mut shell, 2, "display", "flex");
    layout(&mut shell);
    assert_eq!(shell.node_box(4).unwrap().1, 60.0);
    assert_eq!(shell.tap(1.0, 1.0), Some(77));
    assert_eq!(shell.text(3).unwrap(), "retained");
}

#[test]
fn hiding_cancels_active_and_pending_taps_without_disposing_declarations() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut parent = NodeDesc::view(2, 1);
    parent.height = Some(100.0);
    let mut child = NodeDesc::view(3, 2);
    child.height = Some(100.0);
    shell
        .apply_ops(&encode(
            1,
            &[
                Op::Spawn { desc: parent },
                Op::Spawn { desc: child },
                Op::SpawnGesture {
                    gesture: 10,
                    target: 3,
                    kind: 1,
                    index: 0,
                },
                Op::BindGestureEvent {
                    gesture: 10,
                    slot: 4,
                    handler: 77,
                },
                Op::BindGestureEvent {
                    gesture: 10,
                    slot: 3,
                    handler: 78,
                },
                Op::SpawnGesture {
                    gesture: 11,
                    target: 3,
                    kind: 2,
                    index: 1,
                },
                Op::BindGestureEvent {
                    gesture: 11,
                    slot: 5,
                    handler: 79,
                },
            ],
        ))
        .unwrap();
    layout(&mut shell);
    pointer(&mut shell, PointerAction::Down, 20.0, 20.0, 0.0);
    shell.take_gesture_events();
    set(&mut shell, 2, "visibility", "hidden");
    let cancelled = shell.take_gesture_events();
    assert!(cancelled
        .iter()
        .any(|event| event.handler == 78 && event.slot == 3));
    pointer(&mut shell, PointerAction::Up, 20.0, 20.0, 30.0);
    shell.advance_gestures(400.0);
    assert!(!shell
        .take_gesture_events()
        .iter()
        .any(|event| event.slot == 4 || event.slot == 5));
    set(&mut shell, 2, "visibility", "visible");
    pointer(&mut shell, PointerAction::Down, 20.0, 20.0, 500.0);
    pointer(&mut shell, PointerAction::Up, 20.0, 20.0, 530.0);
    shell.take_gesture_events();
    set(&mut shell, 2, "display", "none");
    set(&mut shell, 2, "display", "flex");
    shell.advance_gestures(900.0);
    assert!(!shell
        .take_gesture_events()
        .iter()
        .any(|event| event.slot == 4 || event.slot == 5));
    pointer(&mut shell, PointerAction::Down, 20.0, 20.0, 1000.0);
    pointer(&mut shell, PointerAction::Up, 20.0, 20.0, 1030.0);
    shell.advance_gestures(1400.0);
    let events = shell.take_gesture_events();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.slot == 4 && event.handler == 77)
            .count(),
        1
    );
}

#[test]
fn nested_axis_clips_intersect_for_pointer_hits_and_paint() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut outer = NodeDesc::view(2, 1);
    outer.width = Some(80.0);
    outer.height = Some(80.0);
    let mut inner = NodeDesc::view(3, 2);
    inner.width = Some(120.0);
    inner.height = Some(40.0);
    let mut child = NodeDesc::view(4, 3);
    child.width = Some(160.0);
    child.height = Some(120.0);
    child.background = 0xff00_ff00;
    shell
        .apply_ops(&encode(
            1,
            &[
                Op::Spawn { desc: outer },
                Op::Spawn { desc: inner },
                Op::Spawn { desc: child },
                Op::SpawnGesture {
                    gesture: 10,
                    target: 4,
                    kind: 1,
                    index: 0,
                },
                Op::BindGestureEvent {
                    gesture: 10,
                    slot: 4,
                    handler: 77,
                },
            ],
        ))
        .unwrap();
    set(&mut shell, 3, "position", "absolute");
    set(&mut shell, 4, "position", "absolute");
    set(&mut shell, 2, "overflowX", "hidden");
    set(&mut shell, 3, "overflowY", "hidden");
    layout(&mut shell);
    pointer(&mut shell, PointerAction::Down, 70.0, 30.0, 0.0);
    pointer(&mut shell, PointerAction::Up, 70.0, 30.0, 20.0);
    assert_eq!(
        shell
            .take_gesture_events()
            .iter()
            .filter(|event| event.handler == 77)
            .count(),
        1
    );
    pointer(&mut shell, PointerAction::Down, 90.0, 30.0, 100.0);
    pointer(&mut shell, PointerAction::Up, 90.0, 30.0, 120.0);
    pointer(&mut shell, PointerAction::Down, 70.0, 50.0, 200.0);
    pointer(&mut shell, PointerAction::Up, 70.0, 50.0, 220.0);
    assert!(!shell
        .take_gesture_events()
        .iter()
        .any(|event| event.handler == 77));
    let paint = shell.paint();
    let mut clips: Vec<(f32, f32, f32, f32)> = Vec::new();
    let mut visible_child = None;
    for cmd in paint.iter() {
        match cmd {
            PaintCmd::PushClip { x, y, w, h } => clips.push((*x, *y, *x + *w, *y + *h)),
            PaintCmd::PopClip => {
                clips.pop().unwrap();
            }
            PaintCmd::Fill {
                x, y, w, h, color, ..
            } if *color == bubble_core::Color::from_argb(0xff00_ff00) => {
                visible_child =
                    Some(clips.iter().fold((*x, *y, *x + *w, *y + *h), |rect, clip| {
                        (
                            rect.0.max(clip.0),
                            rect.1.max(clip.1),
                            rect.2.min(clip.2),
                            rect.3.min(clip.3),
                        )
                    }));
            }
            _ => {}
        }
    }
    assert_eq!(visible_child, Some((0.0, 0.0, 80.0, 40.0)));
}

#[test]
fn text_field_hits_respect_clips_and_hidden_overlays() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut parent = NodeDesc::view(2, 1);
    parent.width = Some(60.0);
    parent.height = Some(60.0);
    let mut field = NodeDesc::view(3, 2);
    field.kind = bubble_core::Kind::Field;
    field.width = Some(120.0);
    field.height = Some(120.0);
    field.text = "edit".into();
    shell
        .apply_ops(&encode(
            1,
            &[Op::Spawn { desc: parent }, Op::Spawn { desc: field }],
        ))
        .unwrap();
    set(&mut shell, 3, "position", "absolute");
    set(&mut shell, 2, "overflowX", "hidden");
    layout(&mut shell);
    assert_eq!(shell.field_at(70.0, 20.0), 0);
    assert_eq!(shell.field_at(20.0, 80.0), 3);
    set(&mut shell, 2, "visibility", "hidden");
    assert_eq!(shell.field_at(20.0, 20.0), 0);
    set(&mut shell, 2, "visibility", "visible");
    let mut overlay = NodeDesc::view(4, 1);
    overlay.width = Some(60.0);
    overlay.height = Some(60.0);
    shell.spawn(overlay).unwrap();
    set(&mut shell, 4, "position", "absolute");
    layout(&mut shell);
    assert_eq!(shell.field_at(20.0, 20.0), 0, "topmost view occludes field");
    set(&mut shell, 4, "display", "none");
    assert_eq!(
        shell.field_at(20.0, 20.0),
        3,
        "hidden overlay stops occluding"
    );
}
