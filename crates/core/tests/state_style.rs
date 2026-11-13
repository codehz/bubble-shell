//! Consumer-visible state style behavior: pressed and focused paint overrides.

use bubble_core::{
    encode, encode_set_prop_color, wrapping_layout, Color, Kind, LayoutRequest, NodeDesc, Op,
    PaintCmd, PointerAction, PointerInput, Shell, Value,
};

const BLUE: u32 = 0xff25_63eb;
const RED: u32 = 0xffdc_2626;
const GREEN: u32 = 0xff16_a34a;
const FOCUSED: u32 = 0xff11_1827;
const PRESSED: u32 = 0xff22_2b36;
const BOTH: u32 = 0xff33_3d4a;

fn measure(
    text: &str,
    font_size: f32,
    line_height: f32,
    _weight: u16,
    max_width: Option<f32>,
) -> (f32, f32) {
    let width = text.chars().count() as f32 * font_size * 0.5;
    let width = max_width.map_or(width, |max| width.min(max));
    let line = if line_height > 0.0 {
        line_height
    } else {
        font_size * 1.2
    };
    (width, line)
}

fn input(action: PointerAction, id: u32, x: f32, y: f32, time_ms: f64) -> PointerInput {
    PointerInput {
        action,
        id,
        x,
        y,
        time_ms,
        buttons: u32::from(action != PointerAction::Up),
        pointer_kind: 0,
        path: Vec::new(),
    }
}

/// Every fill color in paint order.
fn fills(shell: &Shell) -> Vec<u32> {
    shell
        .paint()
        .iter()
        .filter_map(|cmd| match cmd {
            PaintCmd::Fill { color, .. } => Some(color.to_argb()),
            _ => None,
        })
        .collect()
}

/// Host layout that records the alignment it was asked for.
fn host(
    keys: &mut Vec<u8>,
    req: &LayoutRequest<'_>,
) -> Result<bubble_core::LayoutSnapshot, String> {
    keys.push(req.key.align);
    Ok(wrapping_layout(req, |line| {
        measure(
            line,
            req.key.font_size,
            req.key.line_height,
            req.key.font_weight,
            None,
        )
        .0
    }))
}

fn style(style: u32, target: u32, index: u32, when: &str) -> Vec<Op> {
    vec![
        Op::SpawnStateStyle {
            style,
            target,
            index,
        },
        Op::SetStateStyleProp {
            style,
            key: "when".into(),
            value: Value::String(when.into()),
        },
    ]
}

fn paint_op(style: u32, key: &str, value: Value) -> Op {
    Op::SetStateStyleProp {
        style,
        key: key.into(),
        value,
    }
}

/// A 200x200 view. Sizes arrive as `SetProp`; the descriptor body has none.
fn button(id: u32, background: u32) -> Vec<Op> {
    let mut node = NodeDesc::view(id, 1);
    node.background = background;
    vec![
        Op::Spawn { desc: node },
        Op::SetProp {
            handle: id,
            key: "width".into(),
            value: Value::Float(200.0),
        },
        Op::SetProp {
            handle: id,
            key: "height".into(),
            value: Value::Float(200.0),
        },
    ]
}

fn pressed_shell() -> Shell {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut ops = button(2, BLUE);
    ops.extend(style(1, 2, 0, "pressed"));
    ops.push(paint_op(1, "backgroundColor", Value::Int(RED as i64)));
    shell.apply_ops(&encode(1, &ops)).unwrap();
    shell.layout(&mut measure).unwrap();
    shell
}

#[test]
fn pressed_overlay_uses_latest_base_and_stops_on_input_end() {
    let mut shell = pressed_shell();
    assert_eq!(fills(&shell), vec![BLUE]);

    shell.pointer(input(PointerAction::Down, 1, 50.0, 50.0, 0.0));
    assert_eq!(fills(&shell), vec![RED], "pressed replaces the base value");
    assert!(
        shell.take_gesture_events().is_empty(),
        "a state style creates no gesture events"
    );

    // The node's own value keeps changing underneath the pressed overlay.
    shell
        .apply_ops(&encode_set_prop_color(2, 2, "backgroundColor", GREEN))
        .unwrap();
    assert_eq!(fills(&shell), vec![RED]);

    shell.pointer(input(PointerAction::Up, 1, 50.0, 50.0, 40.0));
    assert_eq!(fills(&shell), vec![GREEN]);

    // Moving past the tap slop cancels and the touch never revives it.
    shell.pointer(input(PointerAction::Down, 1, 50.0, 50.0, 100.0));
    assert_eq!(fills(&shell), vec![RED]);
    shell.pointer(input(PointerAction::Move, 1, 61.0, 50.0, 120.0));
    assert_eq!(fills(&shell), vec![GREEN]);
    shell.pointer(input(PointerAction::Move, 1, 50.0, 50.0, 140.0));
    shell.pointer(input(PointerAction::Up, 1, 50.0, 50.0, 160.0));
    assert_eq!(fills(&shell), vec![GREEN]);

    // A host cancel drops it.
    shell.pointer(input(PointerAction::Down, 1, 50.0, 50.0, 200.0));
    assert_eq!(fills(&shell), vec![RED]);
    shell.cancel_gestures(bubble_core::REASON_HOST_CANCEL);
    assert_eq!(fills(&shell), vec![GREEN]);

    // A second finger cancels, and the remaining fingers stay tracked.
    shell.pointer(input(PointerAction::Down, 1, 50.0, 50.0, 300.0));
    assert_eq!(fills(&shell), vec![RED]);
    shell.pointer(input(PointerAction::Down, 2, 60.0, 60.0, 310.0));
    assert_eq!(fills(&shell), vec![GREEN]);
    shell.pointer(input(PointerAction::Move, 1, 52.0, 50.0, 315.0));
    assert_eq!(
        fills(&shell),
        vec![GREEN],
        "a held finger does not revive it"
    );
    shell.pointer(input(PointerAction::Up, 2, 60.0, 60.0, 320.0));
    shell.pointer(input(PointerAction::Up, 1, 50.0, 50.0, 330.0));
    assert_eq!(fills(&shell), vec![GREEN]);

    // A long-press winner cancels the press.
    shell
        .apply_ops(&encode(
            3,
            &[
                Op::SpawnGesture {
                    gesture: 5,
                    target: 2,
                    kind: 3,
                    index: 0,
                },
                Op::BindGestureEvent {
                    gesture: 5,
                    slot: 6,
                    handler: 9,
                },
            ],
        ))
        .unwrap();
    shell.pointer(input(PointerAction::Down, 1, 50.0, 50.0, 400.0));
    assert_eq!(fills(&shell), vec![RED]);
    shell.advance_gestures(1000.0);
    assert_eq!(fills(&shell), vec![GREEN], "a long press cancels pressed");
    shell.pointer(input(PointerAction::Up, 1, 50.0, 50.0, 1100.0));
    assert_eq!(fills(&shell), vec![GREEN]);

    // The editing bypass shows the same feedback without touching the
    // recognizer.
    shell.take_gesture_events();
    shell.visual_pointer(input(PointerAction::Down, 7, 50.0, 50.0, 2000.0));
    assert_eq!(fills(&shell), vec![RED]);
    shell.visual_pointer(input(PointerAction::Up, 7, 50.0, 50.0, 2040.0));
    assert_eq!(fills(&shell), vec![GREEN]);
    assert!(shell.take_gesture_events().is_empty());
}

#[test]
fn state_cascade_focus_and_declaration_lifecycle() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut ops = button(2, BLUE);
    ops.push(Op::SetProp {
        handle: 2,
        key: "role".into(),
        value: Value::String("button".into()),
    });
    ops.extend(style(1, 2, 0, "focused"));
    ops.push(paint_op(1, "backgroundColor", Value::Int(FOCUSED as i64)));
    ops.extend(style(2, 2, 1, "pressed"));
    ops.push(paint_op(2, "backgroundColor", Value::Int(PRESSED as i64)));
    ops.extend(style(3, 2, 2, "pressed focused"));
    ops.push(paint_op(3, "backgroundColor", Value::Int(BOTH as i64)));
    shell.apply_ops(&encode(1, &ops)).unwrap();
    shell.layout(&mut measure).unwrap();

    // A matching focused declaration replaces the default focus ring.
    shell.focus(2).unwrap();
    assert_eq!(fills(&shell), vec![FOCUSED]);
    let ring = Color::new(0.30, 0.64, 1.0, 0.95).to_argb();

    // A disabled focused declaration does not take the ring over.
    shell
        .apply_ops(&encode(2, &[paint_op(1, "enabled", Value::Bool(false))]))
        .unwrap();
    assert_eq!(fills(&shell), vec![BLUE, ring, ring, ring, ring]);
    shell
        .apply_ops(&encode(3, &[paint_op(1, "enabled", Value::Bool(true))]))
        .unwrap();
    assert_eq!(fills(&shell), vec![FOCUSED]);

    // Later declarations win; reordering changes the winner immediately.
    shell.pointer(input(PointerAction::Down, 1, 50.0, 50.0, 0.0));
    assert_eq!(fills(&shell), vec![BOTH]);
    shell
        .apply_ops(&encode(
            4,
            &[Op::MoveStateStyle {
                style: 3,
                target: 2,
                index: 0,
            }],
        ))
        .unwrap();
    assert_eq!(fills(&shell), vec![PRESSED]);
    shell
        .apply_ops(&encode(
            5,
            &[Op::MoveStateStyle {
                style: 1,
                target: 2,
                index: 2,
            }],
        ))
        .unwrap();
    assert_eq!(fills(&shell), vec![FOCUSED]);

    // Null clears one patch and uncovers the earlier declaration.
    shell
        .apply_ops(&encode(6, &[paint_op(1, "backgroundColor", Value::Null)]))
        .unwrap();
    assert_eq!(fills(&shell), vec![PRESSED]);

    // Disabling and disposing step the cascade back one layer at a time.
    shell
        .apply_ops(&encode(7, &[paint_op(2, "enabled", Value::Bool(false))]))
        .unwrap();
    assert_eq!(fills(&shell), vec![BOTH]);
    shell
        .apply_ops(&encode(8, &[Op::DisposeStateStyle { style: 3 }]))
        .unwrap();
    assert_eq!(fills(&shell), vec![BLUE]);

    // Disposing the last pressed declaration cleared the live press, and
    // re-enabling does not revive it inside the same touch.
    shell
        .apply_ops(&encode(9, &[paint_op(2, "enabled", Value::Bool(true))]))
        .unwrap();
    assert_eq!(fills(&shell), vec![BLUE]);
    shell.pointer(input(PointerAction::Up, 1, 50.0, 50.0, 60.0));
    shell.pointer(input(PointerAction::Down, 1, 50.0, 50.0, 80.0));
    assert_eq!(fills(&shell), vec![PRESSED]);

    // Hiding the target drops pressed; it does not come back on reveal.
    shell
        .apply_ops(&encode(
            10,
            &[Op::SetProp {
                handle: 2,
                key: "visibility".into(),
                value: Value::String("hidden".into()),
            }],
        ))
        .unwrap();
    assert_eq!(fills(&shell), Vec::<u32>::new());
    shell
        .apply_ops(&encode(
            11,
            &[Op::SetProp {
                handle: 2,
                key: "visibility".into(),
                value: Value::String("visible".into()),
            }],
        ))
        .unwrap();
    assert_eq!(fills(&shell), vec![BLUE]);

    shell
        .apply_ops(&encode(12, &[Op::DisposeNode { handle: 2 }]))
        .unwrap();
    assert_eq!(fills(&shell), Vec::<u32>::new());
    // Declarations died with the target.
    assert!(shell
        .apply_ops(&encode(
            13,
            &[paint_op(2, "backgroundColor", Value::Int(RED as i64))]
        ))
        .is_err());
}

#[test]
fn effective_text_alignment_matches_edit_geometry() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(320.0, 200.0);
    let mut field = NodeDesc::view(2, 1);
    field.kind = Kind::Field;
    field.text = "Edit".into();
    field.color = 0xffff_ffff;
    let mut text = NodeDesc::text(3, 1, "label");
    text.color = 0xff_ff_ff_ff;
    let mut image = NodeDesc::view(4, 1);
    image.kind = Kind::Image;
    let mut ops = vec![
        Op::Spawn { desc: field },
        Op::SetProp {
            handle: 2,
            key: "width".into(),
            value: Value::Float(200.0),
        },
        Op::SetProp {
            handle: 2,
            key: "height".into(),
            value: Value::Float(40.0),
        },
        Op::Spawn { desc: text },
        Op::SetProp {
            handle: 3,
            key: "width".into(),
            value: Value::Float(100.0),
        },
        Op::SetProp {
            handle: 3,
            key: "height".into(),
            value: Value::Float(20.0),
        },
        Op::Spawn { desc: image },
        Op::SetProp {
            handle: 4,
            key: "width".into(),
            value: Value::Float(100.0),
        },
        Op::SetProp {
            handle: 4,
            key: "height".into(),
            value: Value::Float(20.0),
        },
        Op::SetProp {
            handle: 4,
            key: "src".into(),
            value: Value::String("logo.png".into()),
        },
        Op::SetProp {
            handle: 4,
            key: "fit".into(),
            value: Value::String("contain".into()),
        },
    ];
    ops.extend(style(1, 2, 0, "focused"));
    ops.push(paint_op(1, "textAlign", Value::String("right".into())));
    ops.extend(style(2, 3, 0, "pressed"));
    ops.push(paint_op(2, "color", Value::Int(0xff12_3456)));
    ops.extend(style(3, 4, 0, "pressed"));
    ops.push(paint_op(3, "fit", Value::String("cover".into())));
    ops.push(paint_op(3, "radius", Value::Float(6.0)));
    shell.apply_ops(&encode(1, &ops)).unwrap();

    let mut keys: Vec<u8> = Vec::new();
    shell
        .layout_with(&mut measure, &mut |req| host(&mut keys, req))
        .unwrap();
    assert!(keys.is_empty(), "an unfocused field asks for no snapshot");
    shell.focus(2).unwrap();
    shell
        .layout_with(&mut measure, &mut |req| host(&mut keys, req))
        .unwrap();
    assert_eq!(keys, vec![2], "the field lays out with the focused align");
    let box_before = shell.node_box(2).unwrap();

    let align = shell
        .paint()
        .iter()
        .find_map(|cmd| match cmd {
            PaintCmd::Text { align, text, .. } if text == "Edit" => Some(*align),
            _ => None,
        })
        .expect("field text");
    assert_eq!(align, 2, "paint uses the effective alignment");

    let unit = shell
        .unit_at(150.0, 20.0, &mut measure)
        .unwrap()
        .expect("focused field");
    let caret = shell
        .caret_rect_at(unit, &mut measure)
        .unwrap()
        .expect("caret");
    let round_trip = shell
        .unit_at(caret.0 + 1.0, caret.1 + 1.0, &mut measure)
        .unwrap()
        .expect("focused field");
    assert_eq!(round_trip, unit, "caret and hit testing agree");
    assert!(
        caret.0 > 100.0,
        "right alignment moves the caret into the right half"
    );

    // A pressed Text patches its own color, and the sibling Image is
    // untouched by it.
    let label = |shell: &Shell| {
        shell.paint().iter().find_map(|cmd| match cmd {
            PaintCmd::Text { color, text, .. } if text == "label" => Some(color.to_argb()),
            _ => None,
        })
    };
    let art = |shell: &Shell| {
        shell.paint().iter().find_map(|cmd| match cmd {
            PaintCmd::Image { fit, radius, .. } => Some((*fit, *radius)),
            _ => None,
        })
    };
    assert_eq!(label(&shell), Some(0xff_ff_ff_ff));
    assert_eq!(art(&shell), Some((0, 0.0)));
    let box_at = |shell: &Shell, handle: u32| {
        let (x, y, w, h) = shell.node_box(handle).unwrap();
        (x + w / 2.0, y + h / 2.0)
    };
    let (text_x, text_y) = box_at(&shell, 3);
    shell.pointer(input(PointerAction::Down, 1, text_x, text_y, 0.0));
    assert_eq!(label(&shell), Some(0xff12_3456));
    assert_eq!(art(&shell), Some((0, 0.0)), "a sibling keeps its own style");
    shell.pointer(input(PointerAction::Up, 1, text_x, text_y, 20.0));
    assert_eq!(label(&shell), Some(0xff_ff_ff_ff));

    let (art_x, art_y) = box_at(&shell, 4);
    shell.pointer(input(PointerAction::Down, 1, art_x, art_y, 40.0));
    assert_eq!(art(&shell), Some((1, 6.0)));
    shell.pointer(input(PointerAction::Up, 1, art_x, art_y, 60.0));
    assert_eq!(art(&shell), Some((0, 0.0)));

    // Losing focus returns the field to the base alignment.
    shell.focus(0).unwrap();
    shell.layout(&mut measure).unwrap();
    let align = shell
        .paint()
        .iter()
        .find_map(|cmd| match cmd {
            PaintCmd::Text { align, text, .. } if text == "Edit" => Some(*align),
            _ => None,
        })
        .expect("field text");
    assert_eq!(align, 0);
    assert_eq!(shell.node_box(2).unwrap(), box_before);
}

/// A node with an enabled tap family still shows pressed feedback; the click
/// target decides what is covered, not whether feedback happens.
#[test]
fn pressed_feedback_survives_a_tap_family() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut ops = button(2, BLUE);
    ops.extend(style(1, 2, 0, "pressed"));
    ops.push(paint_op(1, "opacity", Value::Float(0.7)));
    ops.extend([
        Op::SpawnGesture {
            gesture: 5,
            target: 2,
            kind: 1,
            index: 0,
        },
        Op::BindGestureEvent {
            gesture: 5,
            slot: 4,
            handler: 9,
        },
    ]);
    shell.apply_ops(&encode(1, &ops)).unwrap();
    shell.layout(&mut measure).unwrap();
    shell.pointer(input(PointerAction::Down, 1, 50.0, 50.0, 0.0));
    let paint = shell.paint();
    let opacities: Vec<f32> = paint
        .iter()
        .filter_map(|cmd| match cmd {
            PaintCmd::PushOpacity { opacity } => Some(*opacity),
            _ => None,
        })
        .collect();
    drop(paint);
    assert_eq!(opacities, vec![0.7], "pressed opacity reaches paint");
}
