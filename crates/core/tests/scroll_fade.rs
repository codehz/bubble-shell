use bubble_core::{encode, spawn_ops, Kind, NodeDesc, Op, PaintCmd, Shell, Value};

fn layout(shell: &mut Shell) {
    shell.layout(&mut |_, _, _, _, _| (0.0, 0.0)).unwrap();
}

fn fade(shell: &Shell) -> Option<(f32, f32, f32, f32)> {
    shell.paint().iter().find_map(|cmd| match cmd {
        PaintCmd::FadeEdges {
            top,
            right,
            bottom,
            left,
            ..
        } => Some((*top, *right, *bottom, *left)),
        _ => None,
    })
}

fn scroller(child_h: f32, fade_edges: f32) -> Shell {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut root = NodeDesc::view(2, 1);
    root.scroll_y = true;
    root.width = Some(100.0);
    root.height = Some(100.0);
    root.flex_shrink = 0.0;
    root.fade_edges = fade_edges;
    root.background = 0xff37_4151;
    let mut child = NodeDesc::view(3, 2);
    child.width = Some(100.0);
    child.height = Some(child_h);
    child.flex_shrink = 0.0;
    shell
        .apply_ops(&encode(1, &[spawn_ops(root), spawn_ops(child)].concat()))
        .unwrap();
    layout(&mut shell);
    shell
}

#[test]
fn fade_edges_only_when_overflow_exists() {
    let shell = scroller(200.0, 16.0);
    let (top, right, bottom, left) = fade(&shell).expect("fade when overflow exists");
    assert_eq!((top, right, left), (0.0, 0.0, 0.0));
    assert_eq!(bottom, 16.0);
    let mut saw_fill = false;
    let mut open = false;
    let mut faded = false;
    let mut closed = false;
    for cmd in shell.paint().iter() {
        match cmd {
            PaintCmd::Fill { color, .. } if color.to_argb() == 0xff37_4151 => {
                assert!(
                    !open,
                    "container background stays outside the content layer"
                );
                saw_fill = true;
            }
            PaintCmd::PushLayer => {
                assert!(saw_fill && !open);
                open = true;
            }
            PaintCmd::FadeEdges { .. } => {
                assert!(open, "fade erases the content layer, not the framebuffer");
                faded = true;
            }
            PaintCmd::PopLayer => {
                assert!(faded);
                open = false;
                closed = true;
            }
            _ => {}
        }
    }
    assert!(closed && !open);
}

#[test]
fn no_fade_edges_when_no_overflow() {
    let shell = scroller(50.0, 16.0);
    assert!(fade(&shell).is_none(), "no fade without overflow");
    assert!(shell.paint().iter().all(|cmd| !matches!(
        cmd,
        PaintCmd::PushLayer | PaintCmd::PopLayer | PaintCmd::FadeEdges { .. }
    )));
}

#[test]
fn fade_edges_clamps_to_scroll_offset() {
    let mut shell = scroller(300.0, 32.0);
    assert!(shell.scroll_path(&[2], 0.0, 10.0).changed());
    let (top, right, bottom, left) = fade(&shell).expect("fade");
    assert_eq!((top, right, bottom, left), (10.0, 0.0, 32.0, 0.0));
}

#[test]
fn fade_edges_prop_round_trips_and_disables_at_zero() {
    let mut shell = scroller(200.0, 0.0);
    assert!(fade(&shell).is_none());
    shell
        .apply_ops(&encode(
            1,
            &[Op::SetProp {
                handle: 2,
                key: "fadeEdges".into(),
                value: Value::Float(12.0),
            }],
        ))
        .unwrap();
    layout(&mut shell);
    let (top, _, bottom, _) = fade(&shell).expect("prop enables fade");
    assert_eq!((top, bottom), (0.0, 12.0));
    shell
        .apply_ops(&encode(
            1,
            &[Op::SetProp {
                handle: 2,
                key: "fadeEdges".into(),
                value: Value::Float(-4.0),
            }],
        ))
        .unwrap();
    layout(&mut shell);
    assert!(fade(&shell).is_none(), "negative fade clamps to disabled");
}

#[test]
fn horizontal_fade_on_scroll_x() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut root = NodeDesc::view(2, 1);
    root.scroll_x = true;
    root.width = Some(100.0);
    root.height = Some(80.0);
    root.flex_shrink = 0.0;
    root.fade_edges = 16.0;
    let mut child = NodeDesc::view(3, 2);
    child.width = Some(260.0);
    child.height = Some(40.0);
    child.flex_shrink = 0.0;
    shell
        .apply_ops(&encode(1, &[spawn_ops(root), spawn_ops(child)].concat()))
        .unwrap();
    layout(&mut shell);
    let (top, right, bottom, left) = fade(&shell).expect("horizontal fade");
    assert_eq!((top, right, bottom, left), (0.0, 16.0, 0.0, 0.0));
    assert!(shell.scroll_path(&[2], 8.0, 0.0).changed());
    assert_eq!(fade(&shell), Some((0.0, 16.0, 0.0, 8.0)));
}

#[test]
fn multiline_field_fades_vertical_overflow() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut field = NodeDesc::view(2, 1);
    field.kind = Kind::Field;
    field.text = "line\nline\nline\nline".into();
    field.width = Some(100.0);
    field.height = Some(40.0);
    field.fade_edges = 16.0;
    shell.spawn(field).unwrap();
    shell
        .apply_ops(&encode(
            1,
            &[Op::SetProp {
                handle: 2,
                key: "multiline".into(),
                value: Value::Bool(true),
            }],
        ))
        .unwrap();
    shell
        .layout(&mut |_, font, _, _, max_width| {
            if max_width.is_some() {
                (80.0, font * 8.0)
            } else {
                (80.0, font)
            }
        })
        .unwrap();
    assert_eq!(fade(&shell), Some((0.0, 0.0, 16.0, 0.0)));
}

#[test]
fn single_line_field_fades_horizontal_overflow() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 200.0);
    let mut field = NodeDesc::view(2, 1);
    field.kind = Kind::Field;
    field.text = "wide".into();
    field.width = Some(40.0);
    field.height = Some(32.0);
    field.fade_edges = 10.0;
    shell.spawn(field).unwrap();
    shell
        .layout(&mut |text, font, _, _, _| (text.len() as f32 * font, font))
        .unwrap();
    assert_eq!(fade(&shell), Some((0.0, 10.0, 0.0, 0.0)));
}
