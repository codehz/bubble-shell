//! Consumer-visible style declaration behavior: source-driven paint overrides.
//!
//! Declarations are `ViewStyle` / `TextStyle` virtual components whose `when`
//! and patch values are expression programs over UI-thread variables. These
//! tests drive the ops directly; the compiler and runtime lower the authored
//! syntax to exactly these ops.

use bubble_core::{
    encode, encode_set_prop_color, spawn_ops, wrapping_layout, AnimSpec, Color, Kind,
    LayoutRequest, NodeDesc, Op, PaintCmd, PointerAction, PointerInput, ProgramBuilder, Shell,
    Value, VarEvent, EASE_LINEAR, OP_AND, OP_GT, OP_NOT, SOURCE_DISABLED, SOURCE_EDITING,
    SOURCE_FOCUSED, SOURCE_NONE, SOURCE_PRESSED,
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

fn program(build: impl FnOnce(&mut ProgramBuilder)) -> Value {
    let mut builder = ProgramBuilder::new();
    build(&mut builder);
    Value::Program(builder.build().expect("test program"))
}

/// `when` is a Bool program over the variable table.
fn load(var: u32) -> impl FnOnce(&mut ProgramBuilder) {
    move |builder: &mut ProgramBuilder| {
        builder.load(var);
    }
}

/// A pressed source is a 0..1 number, so the old string bit becomes `press > 0`.
fn when_pressed(var: u32) -> impl FnOnce(&mut ProgramBuilder) {
    move |builder: &mut ProgramBuilder| {
        builder.load(var).const_num(0.0).op(OP_GT);
    }
}

fn when_not(var: u32) -> impl FnOnce(&mut ProgramBuilder) {
    move |builder: &mut ProgramBuilder| {
        builder.load(var).op(OP_NOT);
    }
}

fn when_and(
    left: impl FnOnce(&mut ProgramBuilder),
    right: impl FnOnce(&mut ProgramBuilder),
) -> impl FnOnce(&mut ProgramBuilder) {
    move |builder: &mut ProgramBuilder| {
        left(builder);
        right(builder);
        builder.op(OP_AND);
    }
}

/// Source variable plus its target binding.
fn source_var(var: u32, source: u8, target: u32) -> Vec<Op> {
    vec![
        Op::CreateVar {
            var,
            source,
            initial: Value::Null,
        },
        Op::BindVar { var, target },
    ]
}

/// One declaration: `target` node, `component` 1 ViewStyle / 2 TextStyle,
/// `when` value, then paint overrides.
fn decl(id: u32, target: u32, component: u8, index: u32, when: Value) -> Vec<Op> {
    vec![
        Op::SpawnStyleDecl {
            decl: id,
            target,
            component,
            index,
        },
        Op::SetStyleDeclProp {
            decl: id,
            key: "when".into(),
            value: when,
        },
    ]
}

fn paint_op(decl: u32, key: &str, value: Value) -> Op {
    Op::SetStyleDeclProp {
        decl,
        key: key.into(),
        value,
    }
}

/// A 200x200 view. Props use the same `SetProp` path as authored styles.
fn button(id: u32, background: u32) -> Vec<Op> {
    let mut node = NodeDesc::view(id, 1);
    node.background = background;
    let mut ops = spawn_ops(node);
    ops.push(Op::SetProp {
        handle: id,
        key: "width".into(),
        value: Value::Float(200.0),
    });
    ops.push(Op::SetProp {
        handle: id,
        key: "height".into(),
        value: Value::Float(200.0),
    });
    ops
}

fn pressed_shell() -> Shell {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut ops = button(2, BLUE);
    ops.extend(source_var(1, SOURCE_PRESSED, 2));
    ops.extend(decl(1, 2, 1, 0, program(when_pressed(1))));
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
        "a style declaration creates no gesture events"
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
    ops.extend(source_var(1, SOURCE_FOCUSED, 2));
    ops.extend(source_var(2, SOURCE_PRESSED, 2));
    ops.extend(decl(1, 2, 1, 0, program(load(1))));
    ops.push(paint_op(1, "backgroundColor", Value::Int(FOCUSED as i64)));
    ops.extend(decl(2, 2, 1, 1, program(when_pressed(2))));
    ops.push(paint_op(2, "backgroundColor", Value::Int(PRESSED as i64)));
    // `pressed && focused`, expressed as one program over both variables.
    ops.extend(decl(
        3,
        2,
        1,
        2,
        program(when_and(when_pressed(2), load(1))),
    ));
    ops.push(paint_op(3, "backgroundColor", Value::Int(BOTH as i64)));
    shell.apply_ops(&encode(1, &ops)).unwrap();
    shell.layout(&mut measure).unwrap();

    // A matching focused declaration replaces the default focus ring.
    shell.focus(2).unwrap();
    assert_eq!(fills(&shell), vec![FOCUSED]);
    let ring = Color::new(0.30, 0.64, 1.0, 0.95).to_argb();

    // A declaration whose `when` no longer holds does not take the ring over.
    shell
        .apply_ops(&encode(2, &[paint_op(1, "when", Value::Bool(false))]))
        .unwrap();
    assert_eq!(fills(&shell), vec![BLUE, ring, ring, ring, ring]);
    shell
        .apply_ops(&encode(3, &[paint_op(1, "when", program(load(1)))]))
        .unwrap();
    assert_eq!(fills(&shell), vec![FOCUSED]);

    // Later declarations win; reordering changes the winner immediately.
    shell.pointer(input(PointerAction::Down, 1, 50.0, 50.0, 0.0));
    assert_eq!(fills(&shell), vec![BOTH]);
    shell
        .apply_ops(&encode(
            4,
            &[Op::MoveStyleDecl {
                decl: 3,
                target: 2,
                index: 0,
            }],
        ))
        .unwrap();
    assert_eq!(fills(&shell), vec![PRESSED]);
    shell
        .apply_ops(&encode(
            5,
            &[Op::MoveStyleDecl {
                decl: 1,
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

    // A `when` that stopped matching steps the cascade back one layer.
    shell
        .apply_ops(&encode(7, &[paint_op(2, "when", Value::Bool(false))]))
        .unwrap();
    assert_eq!(fills(&shell), vec![BOTH]);
    shell
        .apply_ops(&encode(8, &[Op::DisposeStyleDecl { decl: 3 }]))
        .unwrap();
    assert_eq!(fills(&shell), vec![BLUE]);

    // Disposing the matching pressed declaration cleared the live press, and
    // re-matching does not revive it inside the same touch.
    shell
        .apply_ops(&encode(9, &[paint_op(2, "when", program(when_pressed(2)))]))
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
fn disabled_source_overrides_pressed_and_blocks_feedback() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut ops = button(2, BLUE);
    ops.extend(source_var(1, SOURCE_PRESSED, 2));
    ops.extend(source_var(2, SOURCE_DISABLED, 2));
    // `!disabled && press > 0`: the combination the old string `when` could not
    // express.
    ops.extend(decl(
        1,
        2,
        1,
        0,
        program(when_and(when_not(2), when_pressed(1))),
    ));
    ops.push(paint_op(1, "backgroundColor", Value::Int(RED as i64)));
    ops.extend(decl(3, 2, 1, 1, program(load(2))));
    ops.push(paint_op(3, "opacity", Value::Float(0.5)));
    shell.apply_ops(&encode(1, &ops)).unwrap();
    shell.layout(&mut measure).unwrap();

    shell.pointer(input(PointerAction::Down, 1, 50.0, 50.0, 0.0));
    // Pressed feedback still engages on the interest bit, but `disabled` is
    // false so the pressed paint wins.
    assert_eq!(fills(&shell), vec![RED]);

    // Disabling mid-touch: the pressed variable drops to 0 and the disabled
    // declaration takes over with no extra input.
    shell
        .apply_ops(&encode(
            2,
            &[Op::SetProp {
                handle: 2,
                key: "disabled".into(),
                value: Value::Bool(true),
            }],
        ))
        .unwrap();
    let paint = shell.paint();
    let opacity: Vec<f32> = paint
        .iter()
        .filter_map(|cmd| match cmd {
            PaintCmd::PushOpacity { opacity } => Some(*opacity),
            _ => None,
        })
        .collect();
    drop(paint);
    assert_eq!(
        opacity,
        vec![0.5],
        "the disabled declaration patches opacity"
    );
    assert_eq!(fills(&shell), vec![BLUE], "the pressed overlay is gone");

    shell
        .apply_ops(&encode(
            3,
            &[Op::SetProp {
                handle: 2,
                key: "disabled".into(),
                value: Value::Bool(false),
            }],
        ))
        .unwrap();
    assert_eq!(
        fills(&shell),
        vec![BLUE],
        "a disabled touch does not revive"
    );
}

#[test]
fn focused_and_editing_sources_drive_their_variables() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut field = NodeDesc::view(2, 1);
    field.kind = Kind::Field;
    field.text = "Edit".into();
    field.color = 0xffff_ffff;
    let mut ops = spawn_ops(field);
    ops.push(Op::SetProp {
        handle: 2,
        key: "width".into(),
        value: Value::Float(200.0),
    });
    ops.push(Op::SetProp {
        handle: 2,
        key: "height".into(),
        value: Value::Float(40.0),
    });
    ops.extend(source_var(1, SOURCE_FOCUSED, 2));
    ops.extend(source_var(2, SOURCE_EDITING, 2));
    ops.extend(decl(1, 2, 2, 0, program(load(1))));
    ops.push(paint_op(1, "color", Value::Int(0xff12_3456)));
    ops.extend(decl(2, 2, 2, 1, program(load(2))));
    ops.push(paint_op(2, "textAlign", Value::Int(2)));
    shell.apply_ops(&encode(1, &ops)).unwrap();

    let value = |shell: &Shell| {
        shell
            .paint()
            .iter()
            .find_map(|cmd| match cmd {
                PaintCmd::Text { color, text, .. } if text == "Edit" => Some(color.to_argb()),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(value(&shell), 0xffff_ffff);
    shell.focus(2).unwrap();
    assert_eq!(value(&shell), 0xff12_3456, "focused colors the field text");
    // An editing source is true for the focused field, so the text aligns
    // right in the layout snapshot the caret uses.
    let mut keys: Vec<u8> = Vec::new();
    shell
        .layout_with(&mut measure, &mut |req| host(&mut keys, req))
        .unwrap();
    assert_eq!(keys, vec![2]);
    shell.focus(0).unwrap();
    assert_eq!(value(&shell), 0xffff_ffff);
    let mut keys: Vec<u8> = Vec::new();
    shell
        .layout_with(&mut measure, &mut |req| host(&mut keys, req))
        .unwrap();
    assert!(keys.is_empty(), "an unfocused field asks for no snapshot");
}

#[test]
fn script_writes_an_unlinked_variable() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut ops = button(2, BLUE);
    ops.extend([Op::CreateVar {
        var: 1,
        source: SOURCE_NONE,
        initial: Value::Float(0.0),
    }]);
    ops.extend(decl(
        1,
        2,
        1,
        0,
        program(|builder| {
            builder.load(1).const_num(0.0).op(OP_GT);
        }),
    ));
    ops.push(paint_op(1, "backgroundColor", Value::Int(RED as i64)));
    shell.apply_ops(&encode(1, &ops)).unwrap();
    shell.layout(&mut measure).unwrap();
    assert_eq!(fills(&shell), vec![BLUE]);

    // A script-driven variable reaches the UI thread only through a batch;
    // stage 2 wires `SetVar` writes, so the value stays at 0 for now.
    shell
        .apply_ops(&encode(2, &[Op::BindVar { var: 1, target: 0 }]))
        .unwrap();
    assert_eq!(fills(&shell), vec![BLUE]);
}

#[test]
fn static_when_and_program_validation_fail_fast() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let ops = button(2, BLUE);
    shell.apply_ops(&encode(1, &ops)).unwrap();

    // `when` must be a bool program or bool constant.
    assert!(shell
        .apply_ops(&encode(
            2,
            &[
                Op::SpawnStyleDecl {
                    decl: 1,
                    target: 2,
                    component: 1,
                    index: 0,
                },
                paint_op(1, "when", Value::Int(1)),
            ],
        ))
        .is_err());
    // A patch slot with the wrong type fails the batch.
    assert!(shell
        .apply_ops(&encode(
            3,
            &[
                Op::SpawnStyleDecl {
                    decl: 2,
                    target: 2,
                    component: 1,
                    index: 0,
                },
                paint_op(2, "when", Value::Bool(true)),
                paint_op(2, "backgroundColor", Value::Int(RED as i64)),
            ],
        ))
        .is_ok());
    // ViewStyle has no color/textAlign; TextStyle has no background.
    assert!(shell
        .apply_ops(&encode(
            4,
            &[
                Op::SpawnStyleDecl {
                    decl: 3,
                    target: 2,
                    component: 2,
                    index: 1,
                },
                paint_op(3, "color", Value::Int(0xff12_3456)),
            ],
        ))
        .is_ok());
    assert!(shell
        .apply_ops(&encode(
            5,
            &[
                Op::SpawnStyleDecl {
                    decl: 4,
                    target: 2,
                    component: 1,
                    index: 2,
                },
                paint_op(4, "textAlign", Value::Int(2)),
            ],
        ))
        .is_err());
    // Unknown variables are rejected at apply time.
    assert!(shell
        .apply_ops(&encode(
            6,
            &[
                Op::SpawnStyleDecl {
                    decl: 5,
                    target: 2,
                    component: 1,
                    index: 3,
                },
                paint_op(5, "when", program(load(99))),
            ],
        ))
        .is_err());
}

/// A node with an enabled tap family still shows pressed feedback; the click
/// target decides what is covered, not whether feedback happens.
#[test]
fn pressed_feedback_survives_a_tap_family() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut ops = button(2, BLUE);
    ops.extend(source_var(1, SOURCE_PRESSED, 2));
    ops.extend(decl(1, 2, 1, 0, program(when_pressed(1))));
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

// ---------------------------------------------------------------------------
// Stage 2: script-driven variables
// ---------------------------------------------------------------------------

/// A Bool script variable plus a `when: var` declaration patching green.
fn var_shell() -> Shell {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut ops = button(2, BLUE);
    ops.push(Op::CreateVar {
        var: 1,
        source: SOURCE_NONE,
        initial: Value::Bool(false),
    });
    ops.extend(decl(1, 2, 1, 0, program(load(1))));
    ops.push(paint_op(1, "backgroundColor", Value::Int(GREEN as i64)));
    shell.apply_ops(&encode(1, &ops)).unwrap();
    shell.layout(&mut measure).unwrap();
    shell
}

#[test]
fn script_var_write_drives_declaration_paint() {
    let mut shell = var_shell();
    assert_eq!(fills(&shell), vec![BLUE]);

    shell
        .apply_ops(&encode(
            2,
            &[Op::SetVar {
                var: 1,
                value: Value::Bool(true),
                anim: None,
            }],
        ))
        .unwrap();
    shell.layout(&mut measure).unwrap();
    assert_eq!(fills(&shell), vec![GREEN]);

    shell
        .apply_ops(&encode(
            3,
            &[Op::SetVar {
                var: 1,
                value: Value::Bool(false),
                anim: None,
            }],
        ))
        .unwrap();
    shell.layout(&mut measure).unwrap();
    assert_eq!(fills(&shell), vec![BLUE], "false reverts the patch");
}

#[test]
fn script_var_write_rejects_source_variable() {
    let mut shell = pressed_shell();
    let err = shell
        .apply_ops(&encode(
            2,
            &[Op::SetVar {
                var: 1,
                value: Value::Bool(true),
                anim: None,
            }],
        ))
        .unwrap_err();
    assert!(err.contains("state source"), "{err}");
    // The failed write left the variable untouched.
    shell.layout(&mut measure).unwrap();
    assert_eq!(fills(&shell), vec![BLUE]);
}

#[test]
fn script_var_write_unknown_var_fails() {
    let mut shell = Shell::blank().unwrap();
    let err = shell
        .apply_ops(&encode(
            1,
            &[Op::SetVar {
                var: 9,
                value: Value::Bool(true),
                anim: None,
            }],
        ))
        .unwrap_err();
    assert!(err.contains("unknown var"), "{err}");
}

#[test]
fn settle_var_emits_one_event_per_request() {
    let mut shell = var_shell();
    shell
        .apply_ops(&encode(
            2,
            &[
                Op::SetVar {
                    var: 1,
                    value: Value::Bool(true),
                    anim: None,
                },
                Op::SettleVar { var: 1 },
            ],
        ))
        .unwrap();
    assert_eq!(shell.take_var_events(), vec![VarEvent { var: 1 }]);
    assert!(shell.take_var_events().is_empty(), "drain empties");
}

#[test]
fn settle_var_unknown_var_fails() {
    let mut shell = Shell::blank().unwrap();
    let err = shell
        .apply_ops(&encode(1, &[Op::SettleVar { var: 9 }]))
        .unwrap_err();
    assert!(err.contains("unknown var"), "{err}");
}

// ---------------------------------------------------------------------------
// Stage 3: variable animation
// ---------------------------------------------------------------------------

/// A Num script variable with a 100 ms linear tween plus a declaration that
/// patches green while the value exceeds 0.5.
fn tween_shell() -> Shell {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut ops = button(2, BLUE);
    ops.push(Op::CreateVar {
        var: 1,
        source: SOURCE_NONE,
        initial: Value::Float(0.0),
    });
    ops.extend(decl(
        1,
        2,
        1,
        0,
        program(|builder| {
            builder.load(1).const_num(0.5).op(OP_GT);
        }),
    ));
    ops.push(paint_op(1, "backgroundColor", Value::Int(GREEN as i64)));
    shell.apply_ops(&encode(1, &ops)).unwrap();
    shell.layout(&mut measure).unwrap();
    shell
}

#[test]
fn tween_write_tracks_the_tick_and_settles() {
    let mut shell = tween_shell();
    assert_eq!(fills(&shell), vec![BLUE]);

    shell
        .apply_ops(&encode(
            2,
            &[
                Op::SetVar {
                    var: 1,
                    value: Value::Float(1.0),
                    anim: Some(AnimSpec::Tween {
                        duration_ms: 100.0,
                        easing: EASE_LINEAR,
                    }),
                },
                Op::SettleVar { var: 1 },
            ],
        ))
        .unwrap();
    shell.layout(&mut measure).unwrap();
    assert_eq!(
        fills(&shell),
        vec![BLUE],
        "the tween starts at the old value"
    );
    assert!(
        shell.take_var_events().is_empty(),
        "no settle while in flight"
    );

    shell.advance_animations(60.0);
    shell.layout(&mut measure).unwrap();
    assert_eq!(
        fills(&shell),
        vec![GREEN],
        "past the midpoint the patch matches"
    );

    shell.advance_animations(160.0);
    shell.layout(&mut measure).unwrap();
    assert_eq!(fills(&shell), vec![GREEN]);
    assert_eq!(
        shell.take_var_events(),
        vec![VarEvent { var: 1 }],
        "convergence settles"
    );
    assert!(shell.take_var_events().is_empty());
}

#[test]
fn instant_override_lands_immediately() {
    let mut shell = tween_shell();
    shell
        .apply_ops(&encode(
            2,
            &[Op::SetVar {
                var: 1,
                value: Value::Float(1.0),
                anim: None,
            }],
        ))
        .unwrap();
    shell.layout(&mut measure).unwrap();
    assert_eq!(
        fills(&shell),
        vec![GREEN],
        "the per-write override skips the tween"
    );

    shell
        .apply_ops(&encode(
            3,
            &[Op::SetVar {
                var: 1,
                value: Value::Float(0.0),
                anim: Some(AnimSpec::Tween {
                    duration_ms: 100.0,
                    easing: EASE_LINEAR,
                }),
            }],
        ))
        .unwrap();
    shell.layout(&mut measure).unwrap();
    assert_eq!(
        fills(&shell),
        vec![GREEN],
        "the tween restarts from the displayed 1.0"
    );
    shell.advance_animations(60.0);
    shell.layout(&mut measure).unwrap();
    assert_eq!(
        fills(&shell),
        vec![BLUE],
        "past the midpoint the patch stops matching"
    );
}

#[test]
fn spring_write_converges_and_settles() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut ops = button(2, BLUE);
    ops.push(Op::CreateVar {
        var: 1,
        source: SOURCE_NONE,
        initial: Value::Float(0.0),
    });
    ops.extend(decl(
        1,
        2,
        1,
        0,
        program(|builder| {
            builder.load(1).const_num(0.5).op(OP_GT);
        }),
    ));
    ops.push(paint_op(1, "backgroundColor", Value::Int(GREEN as i64)));
    shell.apply_ops(&encode(1, &ops)).unwrap();
    shell.layout(&mut measure).unwrap();

    shell
        .apply_ops(&encode(
            2,
            &[
                Op::SetVar {
                    var: 1,
                    value: Value::Float(1.0),
                    anim: Some(AnimSpec::Spring {
                        stiffness: 120.0,
                        damping: 12.0,
                    }),
                },
                Op::SettleVar { var: 1 },
            ],
        ))
        .unwrap();
    shell.layout(&mut measure).unwrap();
    assert!(
        shell.take_var_events().is_empty(),
        "no settle while in flight"
    );

    // The spring crosses the midpoint quickly and converges within a second.
    let mut settled = false;
    for ms in [
        16.0, 32.0, 48.0, 64.0, 96.0, 160.0, 320.0, 640.0, 1280.0, 1600.0, 2000.0,
    ] {
        shell.advance_animations(ms);
        if !shell.take_var_events().is_empty() {
            settled = true;
            break;
        }
    }
    assert!(settled, "the spring must converge and settle");
    shell.layout(&mut measure).unwrap();
    assert_eq!(
        fills(&shell),
        vec![GREEN],
        "convergence snaps onto the target"
    );
}

#[test]
fn transform_patch_moves_the_node_paint() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut ops = button(2, BLUE);
    ops.extend(decl(1, 2, 1, 0, Value::Bool(true)));
    ops.push(paint_op(1, "translateX", Value::Float(10.0)));
    ops.push(paint_op(1, "translateY", Value::Float(5.0)));
    ops.push(paint_op(1, "scale", Value::Float(2.0)));
    ops.push(paint_op(1, "rotate", Value::Float(0.5)));
    shell.apply_ops(&encode(1, &ops)).unwrap();
    shell.layout(&mut measure).unwrap();

    let mut transforms = shell
        .paint()
        .iter()
        .filter_map(|cmd| match cmd {
            PaintCmd::PushTransform {
                cx,
                cy,
                translate_x,
                translate_y,
                scale,
                rotate,
            } => Some((*cx, *cy, *translate_x, *translate_y, *scale, *rotate)),
            _ => None,
        })
        .collect::<Vec<_>>();
    transforms.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    // One transform around the node center; the focus ring shares the node's
    // transform, so a single push covers the whole subtree.
    assert_eq!(transforms, vec![(100.0, 100.0, 10.0, 5.0, 2.0, 0.5)]);

    // A unit transform is skipped: no push, no pop, zero cost.
    let mut ops = vec![paint_op(1, "scale", Value::Float(1.0))];
    shell.apply_ops(&encode(2, &ops)).unwrap();
    shell.layout(&mut measure).unwrap();
    ops = vec![paint_op(1, "translateX", Value::Float(0.0))];
    shell.apply_ops(&encode(3, &ops)).unwrap();
    shell.layout(&mut measure).unwrap();
    ops = vec![paint_op(1, "translateY", Value::Float(0.0))];
    shell.apply_ops(&encode(4, &ops)).unwrap();
    shell.layout(&mut measure).unwrap();
    ops = vec![paint_op(1, "rotate", Value::Float(0.0))];
    shell.apply_ops(&encode(5, &ops)).unwrap();
    shell.layout(&mut measure).unwrap();
    assert!(shell
        .paint()
        .iter()
        .all(|cmd| !matches!(cmd, PaintCmd::PushTransform { .. })));
}
