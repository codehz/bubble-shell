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
                key: "scrollChainY".into(),
                value: Value::String("contain".into()),
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
