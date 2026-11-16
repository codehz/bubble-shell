//! Consumer-visible gesture and scroll-negotiation behavior.

use bubble_core::{
    encode, encode_events, spawn_ops, wrapping_layout, EditCommand, GestureEvent, Kind,
    LayoutRequest, LayoutSnapshot, NodeDesc, Op, PointerAction, PointerInput, Shell, Value,
};

fn measure(
    text: &str,
    font_size: f32,
    line_height: f32,
    _weight: u16,
    _max: Option<f32>,
) -> (f32, f32) {
    (
        text.chars().count() as f32 * font_size * 0.5,
        if line_height > 0.0 {
            line_height
        } else {
            font_size * 1.2
        },
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
    let view = NodeDesc::view(2, 1);
    shell
        .apply_ops(&encode(
            1,
            &[
                spawn_ops(view),
                vec![
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
                    Op::SetProp {
                        handle: 2,
                        key: "width".into(),
                        value: Value::Float(200.0),
                    },
                    Op::SetProp {
                        handle: 2,
                        key: "height".into(),
                        value: Value::Float(200.0),
                    },
                ],
            ]
            .concat(),
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
fn ui_ticks_do_not_cancel_an_uncontested_tap() {
    let mut shell = tap_shell();
    shell.pointer(input(PointerAction::Down, 50.0, 50.0, 0.0));
    for frame in 1..=10 {
        shell.advance_gestures(f64::from(frame) * 16.0);
    }
    assert!(shell.take_gesture_events().is_empty());
    shell.pointer(input(PointerAction::Up, 50.0, 50.0, 170.0));
    let events = shell.take_gesture_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].slot, 4);
}

#[test]
fn a_long_press_timeout_cancels_the_tap_on_the_same_tick() {
    let mut shell = tap_shell();
    shell
        .apply_ops(&encode(
            2,
            &[
                Op::SpawnGesture {
                    gesture: 11,
                    target: 2,
                    kind: 3,
                    index: 0,
                },
                Op::SetGestureProp {
                    gesture: 11,
                    key: "duration".into(),
                    value: Value::Int(100),
                },
                Op::BindGestureEvent {
                    gesture: 11,
                    slot: 6,
                    handler: 21,
                },
                Op::BindGestureEvent {
                    gesture: 11,
                    slot: 8,
                    handler: 24,
                },
                Op::BindGestureEvent {
                    gesture: 10,
                    slot: 1,
                    handler: 23,
                },
                Op::BindGestureEvent {
                    gesture: 10,
                    slot: 3,
                    handler: 22,
                },
            ],
        ))
        .unwrap();
    shell.pointer(input(PointerAction::Down, 50.0, 50.0, 0.0));
    shell.advance_gestures(120.0);
    let events = shell.take_gesture_events();
    assert!(events
        .iter()
        .any(|event| event.slot == 6 && event.gesture == 11));
    assert!(events.iter().any(|event| event.slot == 3));
    shell.pointer(input(PointerAction::Up, 50.0, 50.0, 140.0));
    let ended = shell.take_gesture_events();
    assert!(ended
        .iter()
        .any(|event| event.slot == 8 && event.gesture == 11));
    assert!(!ended.iter().any(|event| event.slot == 4));
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
    let mut inner = NodeDesc::view(3, 2);
    inner.scroll_y = true;
    inner.flex_shrink = 0.0;
    let mut inner_content = NodeDesc::view(4, 3);
    inner_content.flex_shrink = 0.0;
    let mut spacer = NodeDesc::view(5, 2);
    spacer.flex_shrink = 0.0;
    shell
        .apply_ops(&encode(
            1,
            &[
                spawn_ops(outer),
                vec![
                    Op::SetProp {
                        handle: 2,
                        key: "width".into(),
                        value: Value::Float(100.0),
                    },
                    Op::SetProp {
                        handle: 2,
                        key: "height".into(),
                        value: Value::Float(100.0),
                    },
                ],
                spawn_ops(inner),
                vec![
                    Op::SetProp {
                        handle: 3,
                        key: "width".into(),
                        value: Value::Float(100.0),
                    },
                    Op::SetProp {
                        handle: 3,
                        key: "height".into(),
                        value: Value::Float(50.0),
                    },
                ],
                spawn_ops(inner_content),
                vec![
                    Op::SetProp {
                        handle: 4,
                        key: "width".into(),
                        value: Value::Float(100.0),
                    },
                    Op::SetProp {
                        handle: 4,
                        key: "height".into(),
                        value: Value::Float(80.0),
                    },
                ],
                spawn_ops(spacer),
                vec![
                    Op::SetProp {
                        handle: 5,
                        key: "width".into(),
                        value: Value::Float(100.0),
                    },
                    Op::SetProp {
                        handle: 5,
                        key: "height".into(),
                        value: Value::Float(300.0),
                    },
                ],
            ]
            .concat(),
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
    let mut inner = NodeDesc::view(3, 2);
    inner.scroll_y = true;
    inner.flex_shrink = 0.0;
    let mut content = NodeDesc::view(4, 3);
    content.flex_shrink = 0.0;
    let mut spacer = NodeDesc::view(5, 2);
    spacer.flex_shrink = 0.0;
    shell
        .apply_ops(&encode(
            1,
            &[
                spawn_ops(outer),
                vec![
                    Op::SetProp {
                        handle: 2,
                        key: "width".into(),
                        value: Value::Float(100.0),
                    },
                    Op::SetProp {
                        handle: 2,
                        key: "height".into(),
                        value: Value::Float(100.0),
                    },
                ],
                spawn_ops(inner),
                vec![
                    Op::SetProp {
                        handle: 3,
                        key: "width".into(),
                        value: Value::Float(100.0),
                    },
                    Op::SetProp {
                        handle: 3,
                        key: "height".into(),
                        value: Value::Float(50.0),
                    },
                ],
                spawn_ops(content),
                vec![
                    Op::SetProp {
                        handle: 4,
                        key: "width".into(),
                        value: Value::Float(100.0),
                    },
                    Op::SetProp {
                        handle: 4,
                        key: "height".into(),
                        value: Value::Float(80.0),
                    },
                ],
                spawn_ops(spacer),
                vec![
                    Op::SetProp {
                        handle: 5,
                        key: "width".into(),
                        value: Value::Float(100.0),
                    },
                    Op::SetProp {
                        handle: 5,
                        key: "height".into(),
                        value: Value::Float(300.0),
                    },
                ],
            ]
            .concat(),
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
    assert!(started
        .iter()
        .any(|event| event.slot == 6 && event.gesture == 11));
    shell
        .apply_ops(&encode(3, &[Op::DisposeGesture { gesture: 11 }]))
        .unwrap();
    let events: Vec<GestureEvent> = shell.take_gesture_events();
    let cancel_at = events
        .iter()
        .position(|event| event.slot == 9 && event.gesture == 11)
        .unwrap();
    let ack_at = events
        .iter()
        .position(|event| event.slot == 0 && event.gesture == 11 && event.handler == 0)
        .unwrap();
    assert!(cancel_at < ack_at);
    assert_eq!(
        events
            .iter()
            .filter(|event| event.slot == 0 && event.gesture == 11)
            .count(),
        1
    );
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
    let outer = NodeDesc::view(2, 1);
    let mut child = NodeDesc::view(3, 2);
    child.flex_shrink = 0.0;
    shell
        .apply_ops(&encode(
            1,
            &[
                spawn_ops(outer),
                spawn_ops(child),
                vec![
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
                    Op::SetProp {
                        handle: 3,
                        key: "width".into(),
                        value: Value::Float(100.0),
                    },
                    Op::SetProp {
                        handle: 3,
                        key: "height".into(),
                        value: Value::Float(100.0),
                    },
                ],
            ]
            .concat(),
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

fn finger(action: PointerAction, id: u32, x: f32, y: f32, time_ms: f64) -> PointerInput {
    PointerInput {
        action,
        id,
        x,
        y,
        time_ms,
        buttons: if action == PointerAction::Up || action == PointerAction::Cancel {
            0
        } else {
            1
        },
        pointer_kind: 0,
        path: Vec::new(),
    }
}

fn slot_count(events: &[GestureEvent], gesture: u32, slot: u8) -> usize {
    events
        .iter()
        .filter(|event| event.gesture == gesture && event.slot == slot)
        .count()
}

fn phase_pad() -> Shell {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let view = NodeDesc::view(2, 1);
    shell
        .apply_ops(&encode(
            1,
            &[
                spawn_ops(view),
                vec![
                    Op::SetProp {
                        handle: 2,
                        key: "width".into(),
                        value: Value::Float(200.0),
                    },
                    Op::SetProp {
                        handle: 2,
                        key: "height".into(),
                        value: Value::Float(200.0),
                    },
                ],
            ]
            .concat(),
        ))
        .unwrap();
    shell.layout(&mut measure).unwrap();
    shell
}

fn track(gesture: u32, kind: u32) -> Vec<Op> {
    vec![
        Op::SpawnGesture {
            gesture,
            target: 2,
            kind,
            index: 0,
        },
        Op::BindGestureEvent {
            gesture,
            slot: 6,
            handler: 1,
        },
        Op::BindGestureEvent {
            gesture,
            slot: 7,
            handler: 2,
        },
        Op::BindGestureEvent {
            gesture,
            slot: 8,
            handler: 3,
        },
        Op::BindGestureEvent {
            gesture,
            slot: 9,
            handler: 4,
        },
    ]
}

fn arm_transform(shell: &mut Shell) {
    let mut ops = track(15, 4);
    ops.extend(track(16, 5));
    ops.push(Op::SetGestureProp {
        gesture: 16,
        key: "mode".into(),
        value: Value::String("pan".into()),
    });
    shell.apply_ops(&encode(2, &ops)).unwrap();
    shell.pointer(finger(PointerAction::Down, 1, 30.0, 80.0, 0.0));
    shell.pointer(finger(PointerAction::Move, 1, 50.0, 80.0, 10.0));
    shell.pointer(finger(PointerAction::Down, 2, 70.0, 80.0, 20.0));
    shell.pointer(finger(PointerAction::Move, 2, 130.0, 80.0, 30.0));
}

#[test]
fn transform_lifts_end_once_for_either_finger() {
    for (first, second) in [(1u32, 2u32), (2, 1)] {
        let mut shell = phase_pad();
        arm_transform(&mut shell);
        let started = shell.take_gesture_events();
        assert_eq!(slot_count(&started, 15, 9), 1);
        assert_eq!(slot_count(&started, 15, 8), 0);
        assert_eq!(slot_count(&started, 16, 6), 1);
        assert_eq!(slot_count(&started, 16, 8), 0);
        assert_eq!(slot_count(&started, 16, 9), 0);
        shell.pointer(finger(PointerAction::Up, first, 50.0, 80.0, 40.0));
        shell.pointer(finger(PointerAction::Move, second, 150.0, 96.0, 50.0));
        let ended = shell.take_gesture_events();
        assert_eq!(slot_count(&ended, 16, 8), 1);
        assert_eq!(slot_count(&ended, 16, 9), 0);
        assert_eq!(slot_count(&ended, 15, 8), 0);
        assert_eq!(slot_count(&ended, 15, 9), 0);
        assert_eq!(slot_count(&ended, 16, 7), 0);
        shell.pointer(finger(PointerAction::Up, second, 150.0, 96.0, 60.0));
        assert!(shell.take_gesture_events().is_empty());
    }
}

#[test]
fn transform_host_cancel_suppresses_a_later_end() {
    let mut shell = phase_pad();
    arm_transform(&mut shell);
    let started = shell.take_gesture_events();
    assert_eq!(slot_count(&started, 16, 6), 1);
    shell.pointer(finger(PointerAction::Cancel, 1, 50.0, 80.0, 40.0));
    let canceled = shell.take_gesture_events();
    assert_eq!(slot_count(&canceled, 16, 9), 1);
    assert_eq!(slot_count(&canceled, 16, 8), 0);
    assert_eq!(slot_count(&canceled, 15, 9), 0);
    assert_eq!(slot_count(&canceled, 15, 8), 0);
    shell.pointer(finger(PointerAction::Move, 2, 140.0, 80.0, 50.0));
    shell.pointer(finger(PointerAction::Up, 1, 50.0, 80.0, 60.0));
    shell.pointer(finger(PointerAction::Up, 2, 140.0, 80.0, 70.0));
    assert!(shell.take_gesture_events().is_empty());
}

#[test]
fn transform_end_then_cancel_does_not_emit_again() {
    let mut shell = phase_pad();
    arm_transform(&mut shell);
    assert_eq!(slot_count(&shell.take_gesture_events(), 16, 6), 1);
    shell.pointer(finger(PointerAction::Up, 2, 130.0, 80.0, 40.0));
    let ended = shell.take_gesture_events();
    assert_eq!(slot_count(&ended, 16, 8), 1);
    assert_eq!(slot_count(&ended, 15, 8), 0);
    assert_eq!(slot_count(&ended, 15, 9), 0);
    shell.pointer(finger(PointerAction::Cancel, 1, 50.0, 80.0, 50.0));
    assert!(shell.take_gesture_events().is_empty());
    shell.pointer(finger(PointerAction::Move, 1, 80.0, 90.0, 60.0));
    shell.pointer(finger(PointerAction::Up, 1, 80.0, 90.0, 70.0));
    assert!(shell.take_gesture_events().is_empty());
}

#[test]
fn pan_second_finger_cancel_has_no_end_on_the_other_lift() {
    for (first, second) in [(1u32, 2u32), (2, 1)] {
        let mut shell = phase_pad();
        shell.apply_ops(&encode(2, &track(15, 4))).unwrap();
        shell.pointer(finger(PointerAction::Down, 1, 30.0, 80.0, 0.0));
        shell.pointer(finger(PointerAction::Move, 1, 60.0, 80.0, 10.0));
        shell.pointer(finger(PointerAction::Down, 2, 90.0, 90.0, 20.0));
        let started = shell.take_gesture_events();
        assert_eq!(slot_count(&started, 15, 6), 1);
        assert_eq!(slot_count(&started, 15, 8), 0);
        assert_eq!(slot_count(&started, 15, 9), 0);
        shell.pointer(finger(PointerAction::Up, first, 60.0, 80.0, 30.0));
        shell.pointer(finger(PointerAction::Move, second, 120.0, 100.0, 40.0));
        let canceled = shell.take_gesture_events();
        assert_eq!(slot_count(&canceled, 15, 9), 1);
        assert_eq!(slot_count(&canceled, 15, 8), 0);
        assert_eq!(slot_count(&canceled, 15, 6), 0);
        shell.pointer(finger(PointerAction::Up, second, 120.0, 100.0, 50.0));
        assert!(shell.take_gesture_events().is_empty());
        shell.pointer(finger(PointerAction::Down, 1, 30.0, 80.0, 80.0));
        shell.pointer(finger(PointerAction::Move, 1, 70.0, 80.0, 90.0));
        let again = shell.take_gesture_events();
        assert_eq!(slot_count(&again, 15, 6), 1);
        assert_eq!(slot_count(&again, 15, 8), 0);
    }
}

const CHAR_PX: f32 = 10.0;
const LINE_PX: f32 = 24.0;

fn char_measure(
    text: &str,
    _font: f32,
    line_height: f32,
    _weight: u16,
    _max: Option<f32>,
) -> (f32, f32) {
    let height = if line_height > 0.0 {
        line_height
    } else {
        LINE_PX
    };
    (text.chars().count() as f32 * CHAR_PX, height)
}

fn char_layout(req: &LayoutRequest<'_>) -> Result<LayoutSnapshot, String> {
    Ok(wrapping_layout(req, |line| {
        line.chars().count() as f32 * CHAR_PX
    }))
}

fn set_prop(shell: &mut Shell, epoch: &mut u32, handle: u32, key: &str, value: Value) {
    *epoch += 1;
    shell
        .apply_ops(&encode(
            *epoch,
            &[Op::SetProp {
                handle,
                key: key.into(),
                value,
            }],
        ))
        .unwrap();
}

fn place_field(shell: &mut Shell, epoch: &mut u32, text: &str, top: f32, height: f32) -> u32 {
    let mut desc = NodeDesc::view(0, 1);
    desc.kind = Kind::Field;
    desc.text = text.into();
    desc.width = Some(220.0);
    desc.height = Some(height);
    let handle = shell.next_handle();
    desc.id = handle;
    shell.spawn(desc).unwrap();
    set_prop(
        shell,
        epoch,
        handle,
        "position",
        Value::String("absolute".into()),
    );
    set_prop(shell, epoch, handle, "top", Value::Float(top as f64));
    set_prop(shell, epoch, handle, "start", Value::Float(0.0));
    set_prop(
        shell,
        epoch,
        handle,
        "lineHeight",
        Value::Float(LINE_PX as f64),
    );
    handle
}

fn select_range(shell: &mut Shell, handle: u32, start: u32, end: u32) {
    shell.focus(handle).unwrap();
    shell
        .edit(EditCommand::SetSelection { start, end })
        .unwrap();
    shell.layout(&mut char_measure).unwrap();
}

fn field_sample(action: PointerAction, x: f32, y: f32, time_ms: f64) -> PointerInput {
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

fn handle_margin(handles: [(f32, f32, f32, f32); 2], rect: (f32, f32, f32, f32), x: f32) -> bool {
    for (hx, hy, _, _) in handles {
        let caret_x = hx + 8.0;
        let caret_bottom = hy + 4.0;
        if caret_bottom >= rect.1
            && caret_bottom <= rect.1 + rect.3
            && x >= caret_x - 22.0
            && x < caret_x + 22.0
        {
            return true;
        }
    }
    false
}

fn selection_body_point(shell: &Shell) -> (f32, f32) {
    let handles = shell.selection_handles().expect("touch selection handles");
    for &(x, y, w, h) in shell.selection_rects() {
        let mut sample = x + 0.5;
        while sample < x + w {
            if !handle_margin(handles, (x, y, w, h), sample) {
                return (sample, y + h * 0.5);
            }
            sample += 1.0;
        }
    }
    panic!("selection has no touch body");
}

fn drag_content(shell: &mut Shell, from: (f32, f32), to: (f32, f32)) {
    let mut host = char_layout;
    shell.pointer_with_layout(
        field_sample(PointerAction::Down, from.0, from.1, 0.0),
        Some(&mut host),
    );
    shell.advance_gestures(500.0);
    shell.pointer_with_layout(
        field_sample(PointerAction::Move, to.0, to.1, 520.0),
        Some(&mut host),
    );
    shell.pointer_with_layout(
        field_sample(PointerAction::Up, to.0, to.1, 540.0),
        Some(&mut host),
    );
}

fn two_fields(top_text: &str, bottom_text: &str) -> (Shell, u32, u32) {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(240.0, 200.0);
    let mut epoch = 0;
    let source = place_field(&mut shell, &mut epoch, top_text, 0.0, 40.0);
    let target = place_field(&mut shell, &mut epoch, bottom_text, 50.0, 40.0);
    (shell, source, target)
}

#[test]
fn cross_field_drag_moves_selected_text() {
    let (mut shell, source, target) = two_fields("hello world", "");
    assert_eq!((source, target), (2, 3));
    select_range(&mut shell, source, 6, 11);
    let (sx, sy, _, sh) = shell.node_box(source).unwrap();
    let (tx, ty, _, th) = shell.node_box(target).unwrap();
    assert!(
        (sy - 0.0).abs() < 0.01 && (sh - 40.0).abs() < 0.01,
        "source box"
    );
    assert!(
        (ty - 50.0).abs() < 0.01 && (th - 40.0).abs() < 0.01,
        "target box"
    );
    assert!(shell
        .selection_rects()
        .iter()
        .any(|(x, y, w, h)| { *x <= 60.0 && 60.0 < x + w && *y <= 20.0 && 20.0 < y + h }));
    let from = selection_body_point(&shell);
    assert!(from.0 >= sx && from.1 >= sy && from.1 < sy + sh);
    drag_content(&mut shell, from, (60.0, 70.0));
    assert!(tx <= 60.0 && 70.0 >= ty && 70.0 < ty + th);
    assert_eq!(shell.field(source).unwrap().0, "hello ");
    assert_eq!(shell.field(target).unwrap().0, "world");
    assert_eq!(shell.focused(), target);
    let state = shell.field(target).unwrap().1;
    assert_eq!((state.sel_start(), state.sel_end()), (5, 5));
}

#[test]
fn cross_field_same_field_drop_still_moves_text() {
    let (mut shell, source, _target) = two_fields("hello world", "");
    select_range(&mut shell, source, 6, 11);
    let from = selection_body_point(&shell);
    drag_content(&mut shell, from, (4.0, 12.0));
    assert_eq!(shell.field(source).unwrap().0, "worldhello ");
    assert_eq!(shell.focused(), source);
}

#[test]
fn cross_field_drop_inside_the_selection_restores_it() {
    let (mut shell, source, _target) = two_fields("hello world", "");
    select_range(&mut shell, source, 6, 11);
    let from = selection_body_point(&shell);
    drag_content(&mut shell, from, (90.0, 12.0));
    assert_eq!(shell.field(source).unwrap().0, "hello world");
    let state = shell.field(source).unwrap().1;
    assert_eq!((state.sel_start(), state.sel_end()), (6, 11));
}

#[test]
fn cross_field_drag_replaces_newlines_on_a_single_line_target() {
    let (mut shell, source, target) = two_fields("line1\nline2", "");
    let mut epoch = 0;
    set_prop(
        &mut shell,
        &mut epoch,
        source,
        "multiline",
        Value::Bool(true),
    );
    select_range(&mut shell, source, 0, 11);
    let from = selection_body_point(&shell);
    drag_content(&mut shell, from, (60.0, 70.0));
    assert_eq!(shell.field(source).unwrap().0, "");
    assert_eq!(shell.field(target).unwrap().0, "line1 line2");
    assert_eq!(shell.focused(), target);
}

#[test]
fn cross_field_drag_truncates_to_max_length() {
    let (mut shell, source, target) = two_fields("hello", "ab");
    let mut epoch = 0;
    set_prop(&mut shell, &mut epoch, target, "maxLength", Value::Int(5));
    select_range(&mut shell, source, 0, 5);
    let from = selection_body_point(&shell);
    drag_content(&mut shell, from, (60.0, 70.0));
    assert_eq!(shell.field(source).unwrap().0, "");
    assert_eq!(shell.field(target).unwrap().0, "abhel");
    assert_eq!(shell.focused(), target);
}

#[test]
fn cross_field_drop_off_a_field_restores_the_selection() {
    let (mut shell, source, _target) = two_fields("hello world", "");
    select_range(&mut shell, source, 6, 11);
    let from = selection_body_point(&shell);
    drag_content(&mut shell, from, (60.0, 160.0));
    assert_eq!(shell.field(source).unwrap().0, "hello world");
    let state = shell.field(source).unwrap().1;
    assert_eq!((state.sel_start(), state.sel_end()), (6, 11));
    assert_eq!(shell.focused(), source);
}
