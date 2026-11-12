//! Consumer-visible gesture and scroll-negotiation behavior.

use bubble_core::{
    encode, encode_events, GestureEvent, NodeDesc, Op, PointerAction, PointerInput, Shell, Value,
};

fn measure(text: &str, font_size: f32, line_height: f32, _weight: u16, _max: Option<f32>) -> (f32, f32) {
    (
        text.chars().count() as f32 * font_size * 0.5,
        if line_height > 0.0 { line_height } else { font_size * 1.2 },
    )
}

fn input(action: PointerAction, x: f32, y: f32, time_ms: f64) -> PointerInput {
    PointerInput {
        action,
        id: 1,
        x,
        y,
        time_ms,
        buttons: if action == PointerAction::Up { 0 } else { 1 },
        pointer_kind: 0,
        path: Vec::new(),
    }
}

fn tap_shell() -> Shell {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut view = NodeDesc::view(2, 1);
    view.width = Some(200.0);
    view.height = Some(200.0);
    shell
        .apply_ops(&encode(
            1,
            &[
                Op::Spawn { desc: view },
                Op::SpawnGesture {
                    gesture: 10,
                    target: 2,
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
    shell.layout(&mut measure).unwrap();
    shell
}

#[test]
fn a_tap_fires_only_on_the_up_sample() {
    let mut shell = tap_shell();
    shell.pointer(input(PointerAction::Down, 50.0, 50.0, 0.0));
    assert!(shell.take_gesture_events().is_empty());
    shell.pointer(input(PointerAction::Up, 51.0, 50.0, 40.0));
    let events = shell.take_gesture_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].slot, 4);
    assert_eq!(events[0].handler, 77);
    assert_eq!(events[0].target, 2);
}

#[test]
fn a_drag_cancels_the_press_and_reaches_the_script() {
    let mut shell = tap_shell();
    shell
        .apply_ops(&encode(
            2,
            &[
                Op::BindGestureEvent {
                    gesture: 10,
                    slot: 1,
                    handler: 8,
                },
                Op::BindGestureEvent {
                    gesture: 10,
                    slot: 3,
                    handler: 9,
                },
            ],
        ))
        .unwrap();
    shell.pointer(input(PointerAction::Down, 50.0, 50.0, 0.0));
    shell.pointer(input(PointerAction::Move, 90.0, 50.0, 10.0));
    shell.pointer(input(PointerAction::Up, 90.0, 50.0, 20.0));
    let events = shell.take_gesture_events();
    assert!(events.iter().any(|event| event.slot == 1));
    assert!(events
        .iter()
        .any(|event| event.slot == 3 && event.handler == 9));
}

#[test]
fn nested_scroll_consumes_inner_then_outer_and_contain_truncates() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut outer = NodeDesc::view(2, 1);
    outer.scroll_y = true;
    outer.width = Some(100.0);
    outer.height = Some(100.0);
    let mut inner = NodeDesc::view(3, 2);
    inner.scroll_y = true;
    inner.width = Some(100.0);
    inner.height = Some(50.0);
    inner.flex_shrink = 0.0;
    let mut inner_content = NodeDesc::view(4, 3);
    inner_content.width = Some(100.0);
    inner_content.height = Some(80.0);
    inner_content.flex_shrink = 0.0;
    let mut spacer = NodeDesc::view(5, 2);
    spacer.width = Some(100.0);
    spacer.height = Some(300.0);
    spacer.flex_shrink = 0.0;
    shell
        .apply_ops(&encode(
            1,
            &[
                Op::Spawn { desc: outer },
                Op::Spawn { desc: inner },
                Op::Spawn {
                    desc: inner_content,
                },
                Op::Spawn { desc: spacer },
            ],
        ))
        .unwrap();
    shell.layout(&mut measure).unwrap();
    let outcome = shell.scroll_path(&[3, 2], 0.0, 100.0);
    assert_eq!(outcome.consumed_y, 100.0);
    assert_eq!(shell.scroll_of(3).unwrap().1, 30.0);
    assert_eq!(shell.scroll_of(2).unwrap().1, 70.0);

    shell
        .apply_ops(&encode(
            2,
            &[Op::SetProp {
                handle: 3,
                key: "overscrollY".into(),
                value: Value::Bool(false),
            }],
        ))
        .unwrap();
    let outcome = shell.scroll_path(&[3, 2], 0.0, 40.0);
    assert_eq!(outcome.consumed_y, 0.0);
    assert!(outcome.truncated_y);
    assert_eq!(outcome.remaining_y, 0.0);
}

#[test]
fn reverse_scroll_starts_at_the_deepest_container() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut outer = NodeDesc::view(2, 1);
    outer.scroll_y = true;
    outer.width = Some(100.0);
    outer.height = Some(100.0);
    let mut inner = NodeDesc::view(3, 2);
    inner.scroll_y = true;
    inner.width = Some(100.0);
    inner.height = Some(50.0);
    inner.flex_shrink = 0.0;
    let mut content = NodeDesc::view(4, 3);
    content.width = Some(100.0);
    content.height = Some(80.0);
    content.flex_shrink = 0.0;
    let mut spacer = NodeDesc::view(5, 2);
    spacer.width = Some(100.0);
    spacer.height = Some(300.0);
    spacer.flex_shrink = 0.0;
    shell
        .apply_ops(&encode(
            1,
            &[
                Op::Spawn { desc: outer },
                Op::Spawn { desc: inner },
                Op::Spawn { desc: content },
                Op::Spawn { desc: spacer },
            ],
        ))
        .unwrap();
    shell.layout(&mut measure).unwrap();
    shell.scroll_path(&[3, 2], 0.0, 100.0);
    assert_eq!(shell.scroll_of(3).unwrap().1, 30.0);
    assert_eq!(shell.scroll_of(2).unwrap().1, 70.0);
    let outcome = shell.scroll_path(&[3, 2], 0.0, -20.0);
    assert_eq!(outcome.consumed_y, -20.0);
    assert_eq!(shell.scroll_of(3).unwrap().1, 10.0);
    assert_eq!(shell.scroll_of(2).unwrap().1, 70.0);
}

#[test]
fn disposing_a_gesture_cancels_an_active_sequence() {
    let mut shell = tap_shell();
    shell
        .apply_ops(&encode(
            2,
            &[
                Op::SpawnGesture {
                    gesture: 11,
                    target: 2,
                    kind: 4,
                    index: 0,
                },
                Op::BindGestureEvent {
                    gesture: 11,
                    slot: 6,
                    handler: 5,
                },
                Op::BindGestureEvent {
                    gesture: 11,
                    slot: 9,
                    handler: 6,
                },
            ],
        ))
        .unwrap();
    shell.pointer(input(PointerAction::Down, 50.0, 50.0, 0.0));
    shell.pointer(input(PointerAction::Move, 80.0, 50.0, 10.0));
    let started = shell.take_gesture_events();
    assert!(started.iter().any(|event| event.slot == 6 && event.gesture == 11));
    shell
        .apply_ops(&encode(3, &[Op::DisposeGesture { gesture: 11 }]))
        .unwrap();
    let events: Vec<GestureEvent> = shell.take_gesture_events();
    assert!(events
        .iter()
        .any(|event| event.slot == 9 && event.gesture == 11));
}

#[test]
fn pan_reports_start_update_and_end_geometry() {
    let mut shell = tap_shell();
    shell
        .apply_ops(&encode(
            2,
            &[
                Op::SpawnGesture {
                    gesture: 12,
                    target: 2,
                    kind: 4,
                    index: 0,
                },
                Op::BindGestureEvent {
                    gesture: 12,
                    slot: 6,
                    handler: 1,
                },
                Op::BindGestureEvent {
                    gesture: 12,
                    slot: 7,
                    handler: 2,
                },
                Op::BindGestureEvent {
                    gesture: 12,
                    slot: 8,
                    handler: 3,
                },
                Op::SetGestureProp {
                    gesture: 12,
                    key: "axis".into(),
                    value: Value::String("x".into()),
                },
            ],
        ))
        .unwrap();
    shell.pointer(input(PointerAction::Down, 20.0, 50.0, 0.0));
    shell.pointer(input(PointerAction::Move, 40.0, 50.0, 10.0));
    shell.pointer(input(PointerAction::Move, 70.0, 50.0, 20.0));
    shell.pointer(input(PointerAction::Up, 70.0, 50.0, 30.0));
    let events = shell.take_gesture_events();
    let start = events.iter().find(|event| event.slot == 6).unwrap();
    assert_eq!(start.translation_x, 20.0);
    let update = events.iter().find(|event| event.slot == 7).unwrap();
    assert_eq!(update.translation_x, 50.0);
    assert_eq!(update.delta_x, 30.0);
    let end = events.iter().find(|event| event.slot == 8).unwrap();
    assert_eq!(end.translation_x, 50.0);
}

#[test]
fn transform_scale_is_stable_when_the_baseline_has_no_distance() {
    let mut shell = tap_shell();
    shell
        .apply_ops(&encode(
            2,
            &[
                Op::SpawnGesture {
                    gesture: 13,
                    target: 2,
                    kind: 5,
                    index: 0,
                },
                Op::SetGestureProp {
                    gesture: 13,
                    key: "mode".into(),
                    value: Value::String("pan,pinch".into()),
                },
                Op::BindGestureEvent {
                    gesture: 13,
                    slot: 6,
                    handler: 1,
                },
                Op::BindGestureEvent {
                    gesture: 13,
                    slot: 7,
                    handler: 2,
                },
            ],
        ))
        .unwrap();
    let mut first = input(PointerAction::Down, 50.0, 50.0, 0.0);
    first.id = 1;
    shell.pointer(first);
    let mut second = input(PointerAction::Down, 50.0, 50.0, 5.0);
    second.id = 2;
    shell.pointer(second);
    let mut moved = input(PointerAction::Move, 70.0, 50.0, 10.0);
    moved.id = 2;
    shell.pointer(moved);
    let mut moved = input(PointerAction::Move, 90.0, 50.0, 15.0);
    moved.id = 2;
    shell.pointer(moved);
    let events = shell.take_gesture_events();
    assert!(events.iter().any(|event| event.slot == 6));
    for event in &events {
        assert!(event.scale.is_finite());
        assert!(event.rotation.is_finite());
    }
}

#[test]
fn a_second_finger_without_a_transform_cancels_the_started_pan() {
    let mut shell = tap_shell();
    shell
        .apply_ops(&encode(
            2,
            &[
                Op::SpawnGesture {
                    gesture: 15,
                    target: 2,
                    kind: 4,
                    index: 0,
                },
                Op::BindGestureEvent {
                    gesture: 15,
                    slot: 6,
                    handler: 1,
                },
                Op::BindGestureEvent {
                    gesture: 15,
                    slot: 9,
                    handler: 2,
                },
            ],
        ))
        .unwrap();
    shell.pointer(input(PointerAction::Down, 20.0, 50.0, 0.0));
    shell.pointer(input(PointerAction::Move, 60.0, 50.0, 10.0));
    assert!(shell
        .take_gesture_events()
        .iter()
        .any(|event| event.slot == 6));
    let mut second = input(PointerAction::Down, 100.0, 100.0, 20.0);
    second.id = 2;
    shell.pointer(second);
    let mut lift = input(PointerAction::Up, 100.0, 100.0, 30.0);
    lift.id = 2;
    shell.pointer(lift);
    let events = shell.take_gesture_events();
    assert!(events
        .iter()
        .any(|event| event.slot == 9 && event.gesture == 15));
}

#[test]
fn a_child_tap_fires_immediately_despite_an_ancestor_double_tap() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut outer = NodeDesc::view(2, 1);
    outer.width = Some(200.0);
    outer.height = Some(200.0);
    let mut child = NodeDesc::view(3, 2);
    child.width = Some(100.0);
    child.height = Some(100.0);
    child.flex_shrink = 0.0;
    shell
        .apply_ops(&encode(
            1,
            &[
                Op::Spawn { desc: outer },
                Op::Spawn { desc: child },
                Op::SpawnGesture {
                    gesture: 16,
                    target: 3,
                    kind: 1,
                    index: 0,
                },
                Op::BindGestureEvent {
                    gesture: 16,
                    slot: 4,
                    handler: 1,
                },
                Op::SpawnGesture {
                    gesture: 17,
                    target: 2,
                    kind: 2,
                    index: 0,
                },
                Op::BindGestureEvent {
                    gesture: 17,
                    slot: 5,
                    handler: 2,
                },
            ],
        ))
        .unwrap();
    shell.layout(&mut measure).unwrap();
    shell.pointer(input(PointerAction::Down, 20.0, 20.0, 0.0));
    shell.pointer(input(PointerAction::Up, 20.0, 20.0, 20.0));
    let events = shell.take_gesture_events();
    assert!(events.iter().any(|event| event.slot == 4));
    shell.advance_gestures(1000.0);
    assert!(shell
        .take_gesture_events()
        .iter()
        .all(|event| event.slot != 4));
}

#[test]
fn negotiation_trace_is_opt_in() {
    let mut shell = tap_shell();
    shell
        .apply_ops(&encode(
            2,
            &[
                Op::SpawnGesture {
                    gesture: 18,
                    target: 2,
                    kind: 4,
                    index: 0,
                },
                Op::BindGestureEvent {
                    gesture: 18,
                    slot: 6,
                    handler: 1,
                },
            ],
        ))
        .unwrap();
    shell.pointer(input(PointerAction::Down, 20.0, 50.0, 0.0));
    shell.pointer(input(PointerAction::Move, 60.0, 50.0, 10.0));
    assert!(shell.take_gesture_trace().is_empty());
    shell.pointer(input(PointerAction::Up, 60.0, 50.0, 20.0));
    shell.set_gesture_trace(true);
    shell.pointer(input(PointerAction::Down, 20.0, 50.0, 100.0));
    shell.pointer(input(PointerAction::Move, 60.0, 50.0, 110.0));
    let lines = shell.take_gesture_trace();
    assert!(lines.iter().any(|line| line.contains("accept axes=")));
    shell.set_gesture_trace(false);
    assert!(shell.take_gesture_trace().is_empty());
}

#[test]
fn encoded_batch_carries_the_handler_and_slot() {
    let events = vec![GestureEvent {
        slot: 4,
        gesture: 10,
        sequence: 1,
        handler: 77,
        target: 2,
        ..GestureEvent::new(40.0)
    }];
    let bytes = encode_events(&events);
    assert_eq!(&bytes[..4], b"BSGE");
    assert_eq!(bytes[10], 4);
}
