use bubble_core::{
    encode, encode_set_prop_color, encode_set_text, validate_bundle_hashes, NodeDesc, Op, Shell,
};

fn measure(
    text: &str,
    font_size: f32,
    line_height: f32,
    weight: u16,
    max_width: Option<f32>,
) -> (f32, f32) {
    let _ = weight;
    let line = if line_height > 0.0 {
        line_height
    } else {
        font_size * 1.2
    };
    let width = text.chars().count() as f32 * font_size * 0.5;
    match max_width {
        Some(max) if max > 0.0 && width > max => {
            let lines = (width / max).ceil().max(1.0);
            (max, lines * line)
        }
        _ => (width, line),
    }
}

fn demo_ops() -> Vec<Op> {
    let mut header = NodeDesc::text(2, 1, "Count: 0");
    header.font_size = 24.0;
    header.color = 0xffff_ffff;

    let mut button = NodeDesc::view(3, 1);
    button.background = 0xff3b_82f6;
    button.radius = 8.0;
    button.handler = 1;
    button.padding = [12.0, 8.0, 12.0, 8.0];

    let mut label = NodeDesc::text(4, 3, "Increment");
    label.font_size = 16.0;
    label.color = 0xffff_ffff;

    let mut scroller = NodeDesc::view(5, 1);
    scroller.scroll_y = true;
    scroller.flex_grow = 1.0;
    scroller.flex_shrink = 1.0;

    let column = NodeDesc::view(6, 5);
    let mut ops = vec![
        Op::Spawn { desc: header },
        Op::Spawn { desc: button },
        Op::Spawn { desc: label },
        Op::Spawn { desc: scroller },
        Op::Spawn { desc: column },
    ];
    for index in 0..30 {
        let mut row = NodeDesc::text(7 + index, 6, format!("Row {index:02}"));
        row.font_size = 16.0;
        row.color = 0xffdd_dddd;
        row.padding = [0.0, 4.0, 0.0, 4.0];
        ops.push(Op::Spawn { desc: row });
    }
    ops
}

fn laid_out() -> Shell {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(320.0, 480.0);
    shell.apply_ops(&encode(1, &demo_ops())).unwrap();
    shell.layout(&mut measure).unwrap();
    shell
}

#[test]
fn spawned_nodes_accept_updates() {
    let mut shell = laid_out();
    let batch = encode_set_text(7, 2, "Count: 3");
    assert_eq!(shell.apply_ops(&batch).unwrap(), 7);
    assert_eq!(shell.text(2).unwrap(), "Count: 3");

    let color = encode_set_prop_color(8, 3, "backgroundColor", 0xff00_0000);
    assert_eq!(shell.apply_ops(&color).unwrap(), 8);
    shell.apply_ops(&encode_set_text(9, 3, "nope")).unwrap_err();
}

#[test]
fn rejects_bad_op_magic_and_text_on_a_view() {
    assert!(bubble_core::decode_ops(b"NOPE").is_err());
    let mut shell = Shell::blank().unwrap();
    assert!(shell.apply_ops(&encode_set_text(1, 1, "x")).is_err());
}

#[test]
fn click_scroll_and_paint() {
    let mut shell = laid_out();
    let (_x, y, w, h) = shell.node_box(3).unwrap();
    assert!(w > 0.0 && h > 0.0);
    let (lx, ly, _, lh) = shell.node_box(4).unwrap();
    assert!(
        ly >= y && ly + lh <= y + h + 1.0,
        "label {ly} outside button {y} {h}"
    );
    assert!(shell.node_box(2).unwrap().1 < 20.0, "header double-padded");
    assert_eq!(shell.tap(lx + 1.0, ly + lh / 2.0), Some(1));

    let (sx, sy, sw, sh) = shell.node_box(5).unwrap();
    assert!(sh > 0.0 && sh < 480.0);
    let before = shell.node_box(7).unwrap().1;
    assert!(shell.scroll_wheel(sx + sw / 2.0, sy + sh / 2.0, 0.0, 40.0));
    shell.layout(&mut measure).unwrap();
    let after = shell.node_box(7).unwrap().1;
    assert!(after < before);
    assert!(shell.scroll_of(5).unwrap().1 > 0.0);

    let paint = shell.paint();
    assert!(paint
        .iter()
        .any(|cmd| matches!(cmd, bubble_core::PaintCmd::Text { text, .. } if text == "Count: 0")));
    assert!(paint
        .iter()
        .any(|cmd| matches!(cmd, bubble_core::PaintCmd::PushClip { .. })));
}
#[test]
fn spawn_at_prepends_and_refuses_a_retired_handle() {
    let mut shell = laid_out();
    let handle = shell.next_handle();
    let batch = encode(
        2,
        &[
            Op::SpawnAt {
                desc: NodeDesc::text(handle, 6, ""),
                index: 0,
            },
            Op::SetText {
                handle,
                text: "Added 1".into(),
            },
        ],
    );
    shell.apply_ops(&batch).unwrap();
    shell.layout(&mut measure).unwrap();
    assert_eq!(shell.text(handle).unwrap(), "Added 1");
    assert!(shell
        .paint()
        .iter()
        .any(|cmd| matches!(cmd, bubble_core::PaintCmd::Text { text, .. } if text == "Added 1")));
    shell
        .apply_ops(&encode(3, &[Op::DisposeNode { handle }]))
        .unwrap();
    assert!(shell.text(handle).is_err());
    let again = encode(
        4,
        &[Op::SpawnAt {
            desc: NodeDesc::text(handle, 6, ""),
            index: 0,
        }],
    );
    assert!(shell.apply_ops(&again).is_err());
}
#[test]
fn move_reorders_and_rejects_a_cycle() {
    let mut shell = laid_out();
    let first = shell.next_handle();
    shell
        .apply_ops(&encode(
            2,
            &[
                Op::SpawnAt {
                    desc: NodeDesc::text(first, 6, "First"),
                    index: 0,
                },
                Op::SpawnAt {
                    desc: NodeDesc::text(first + 1, 6, "Second"),
                    index: 1,
                },
            ],
        ))
        .unwrap();
    shell
        .apply_ops(&encode(
            3,
            &[Op::MoveNode {
                handle: first,
                parent: 6,
                index: 1,
            }],
        ))
        .unwrap();
    shell.layout(&mut measure).unwrap();
    let (_, first_y, _, _) = shell.node_box(first).unwrap();
    let (_, second_y, _, _) = shell.node_box(first + 1).unwrap();
    assert!(second_y < first_y);
    assert!(shell
        .apply_ops(&encode(
            4,
            &[Op::MoveNode {
                handle: 6,
                parent: first,
                index: 0,
            }],
        ))
        .is_err());
}
#[test]
fn bind_event_hits_the_node_until_cleared() {
    let mut shell = laid_out();
    let handle = shell.next_handle();
    shell
        .apply_ops(&encode(
            2,
            &[
                Op::SpawnAt {
                    desc: NodeDesc::text(handle, 6, "bound"),
                    index: 0,
                },
                Op::BindEvent { handle, handler: 9 },
            ],
        ))
        .unwrap();
    shell.layout(&mut measure).unwrap();
    let (x, y, w, h) = shell.node_box(handle).unwrap();
    assert_eq!(shell.tap(x + w / 2.0, y + h / 2.0), Some(9));
    shell
        .apply_ops(&encode(3, &[Op::BindEvent { handle, handler: 0 }]))
        .unwrap();
    assert_eq!(shell.tap(x + w / 2.0, y + h / 2.0), None);
}
#[test]
fn scroll_to_clamps_and_rejects_a_plain_node() {
    let mut shell = laid_out();
    shell
        .apply_ops(&encode(
            2,
            &[Op::ScrollTo {
                handle: 5,
                x: 0.0,
                y: 10_000.0,
            }],
        ))
        .unwrap();
    let scrolled = shell.scroll_of(5).unwrap();
    assert!(scrolled.1 > 0.0);
    shell
        .apply_ops(&encode(
            3,
            &[Op::ScrollTo {
                handle: 5,
                x: 0.0,
                y: 0.0,
            }],
        ))
        .unwrap();
    assert_eq!(shell.scroll_of(5).unwrap(), (0.0, 0.0));
    assert!(shell
        .apply_ops(&encode(
            4,
            &[Op::ScrollTo {
                handle: 2,
                x: 1.0,
                y: 1.0,
            }],
        ))
        .is_err());
}

#[test]
fn scroll_axes_are_independent() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 120.0);
    let mut scroller = NodeDesc::view(2, 1);
    scroller.scroll_x = true;
    scroller.scroll_y = true;
    scroller.width = Some(100.0);
    scroller.height = Some(60.0);
    let mut lane = NodeDesc::view(3, 2);
    lane.width = Some(400.0);
    lane.height = Some(200.0);
    lane.flex_shrink = 0.0;
    shell
        .apply_ops(&encode(
            1,
            &[Op::Spawn { desc: scroller }, Op::Spawn { desc: lane }],
        ))
        .unwrap();
    shell.layout(&mut measure).unwrap();
    assert_eq!(shell.scroll_of(2).unwrap(), (0.0, 0.0));

    let (sx, sy, sw, sh) = shell.node_box(2).unwrap();
    let (lx, ly, _, _) = shell.node_box(3).unwrap();
    assert!(shell.scroll_wheel(sx + sw / 2.0, sy + sh / 2.0, 12.0, 7.0));
    assert_eq!(shell.scroll_of(2).unwrap(), (12.0, 7.0));
    shell.layout(&mut measure).unwrap();
    assert_eq!(shell.node_box(3).unwrap().0, lx - 12.0);
    assert_eq!(shell.node_box(3).unwrap().1, ly - 7.0);

    shell
        .apply_ops(&encode(
            2,
            &[Op::ScrollTo {
                handle: 2,
                x: 10_000.0,
                y: 10_000.0,
            }],
        ))
        .unwrap();
    assert_eq!(shell.scroll_of(2).unwrap(), (300.0, 140.0));
    shell
        .apply_ops(&encode(
            3,
            &[Op::ScrollTo {
                handle: 2,
                x: -5.0,
                y: -5.0,
            }],
        ))
        .unwrap();
    assert_eq!(shell.scroll_of(2).unwrap(), (0.0, 0.0));

    let mut vertical_only = NodeDesc::view(4, 1);
    vertical_only.scroll_y = true;
    vertical_only.width = Some(100.0);
    vertical_only.height = Some(40.0);
    let mut tall = NodeDesc::view(5, 4);
    tall.height = Some(200.0);
    tall.flex_shrink = 0.0;
    shell
        .apply_ops(&encode(
            4,
            &[Op::Spawn {
                desc: vertical_only,
            }],
        ))
        .unwrap();
    shell
        .apply_ops(&encode(5, &[Op::Spawn { desc: tall }]))
        .unwrap();
    shell.layout(&mut measure).unwrap();
    shell
        .apply_ops(&encode(
            6,
            &[Op::ScrollTo {
                handle: 4,
                x: 50.0,
                y: 20.0,
            }],
        ))
        .unwrap();
    assert_eq!(shell.scroll_of(4).unwrap(), (0.0, 20.0));
}

#[test]
fn manifest_checks_hashes_and_placeholder_signature() {
    let file = b"hello";
    let hash = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    let ok = format!(
        r#"{{"schema":1,"signature":"cGxhY2Vob2xkZXI","files":[{{"path":"main.js","sha256":"{hash}","size":5}}]}}"#
    );
    validate_bundle_hashes(&ok, &[("main.js", file)]).unwrap();
    validate_bundle_hashes(&ok.replace("cGxhY2Vob2xkZXI", ""), &[("main.js", file)]).unwrap_err();
    let bad = ok.replace(hash, &"ab".repeat(32));
    validate_bundle_hashes(&bad, &[("main.js", file)]).unwrap_err();
    let escape = ok.replace("main.js", "../main.js");
    validate_bundle_hashes(&escape, &[("../main.js", file)]).unwrap_err();
}

#[test]
fn signal_commit_updates_text_and_scrolls() {
    use bubble_core::Turn;

    let mut shell = Shell::blank().unwrap();
    let mut turn = Turn::new();
    let count = turn.alloc_signal();
    struct Host {
        turn: *mut bubble_core::Turn,
        count: u32,
        header: u32,
        value: f64,
    }
    impl bubble_core::Sched for Host {
        fn run(&mut self, _: u32) -> i32 {
            let turn = unsafe { &mut *self.turn };
            turn.read(self.count, self).unwrap();
            let value = self.value;
            turn.push(bubble_core::Op::SetText {
                handle: self.header,
                text: format!("Count: {value}"),
            });
            0
        }
        fn cleanup(&mut self, _: u32) {}
    }
    let mut header = NodeDesc::text(0, 1, "Count: 0");
    header.font_size = 24.0;
    let header = turn.spawn(header).unwrap();
    let mut button = NodeDesc::view(0, 1);
    button.handler = 1;
    button.background = 0xff3b_82f6;
    button.height = Some(32.0);
    let button = turn.spawn(button).unwrap();
    let mut scroller = NodeDesc::view(0, 1);
    scroller.scroll_y = true;
    scroller.flex_grow = 1.0;
    scroller.height = Some(80.0);
    let scroller = turn.spawn(scroller).unwrap();
    let mut row = NodeDesc::text(0, scroller, "row");
    row.font_size = 40.0;
    turn.spawn(row).unwrap();
    let mut row = NodeDesc::text(0, scroller, "row");
    row.font_size = 40.0;
    turn.spawn(row).unwrap();
    shell.apply_ops(&turn.commit().unwrap()).unwrap();
    assert_eq!((header, button, scroller), (2, 3, 4));

    let mut host = Host {
        turn: &mut turn as *mut bubble_core::Turn,
        count,
        header,
        value: 0.0,
    };
    let binding = turn
        .alloc_computation(bubble_core::ComputationKind::Effect)
        .unwrap();
    turn.mount_computation(binding, &mut host).unwrap();
    let batch = turn.commit().unwrap();
    let (_, ops) = bubble_core::decode_ops(&batch).unwrap();
    assert_eq!(ops.len(), 1);
    shell.apply_ops(&batch).unwrap();

    shell.set_viewport(200.0, 120.0);
    shell
        .layout(&mut |text, font, line_height, _weight, max| {
            let width = text.chars().count() as f32 * font * 0.5;
            let width = max.map(|max| width.min(max)).unwrap_or(width);
            let line = if line_height > 0.0 {
                line_height
            } else {
                font * 1.2
            };
            (width, line)
        })
        .unwrap();
    let (bx, by, _, _) = shell.node_box(button).unwrap();
    assert_eq!(shell.tap(bx + 1.0, by + 1.0), Some(1));
    host.value = 1.0;
    turn.notify(count, &mut host).unwrap();
    shell.apply_ops(&turn.commit().unwrap()).unwrap();
    assert_eq!(shell.text(header).unwrap(), "Count: 1");
    let (x, y, _, _) = shell.node_box(scroller).unwrap();
    assert!(shell.scroll_wheel(x + 1.0, y + 1.0, 0.0, 50.0));
    assert!(shell.scroll_of(scroller).unwrap().1 > 0.0);
}

#[test]
fn every_runtime_property_is_dispatchable() {
    use bubble_core::{Prop, Turn, Value, SET_PROPS};

    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 120.0);
    let mut turn = Turn::new();
    let view = turn.spawn(NodeDesc::view(0, 1)).unwrap();
    shell.apply_ops(&turn.commit().unwrap()).unwrap();

    for key in SET_PROPS {
        assert!(Prop::parse(key).is_some(), "{key} is not parseable");
        let value = match *key {
            "color" | "backgroundColor" => Value::Int(0xff00_0000),
            "scrollX" | "scrollY" | "row" => Value::Bool(true),
            "textAlign" => Value::String("center".into()),
            "alignItems" => Value::String("center".into()),
            "justifyContent" => Value::String("space-between".into()),
            "position" => Value::String("absolute".into()),
            "src" => Value::String("logo.png".into()),
            "fit" => Value::String("cover".into()),
            "scrollChainX" | "scrollChainY" => Value::String("contain".into()),
            "top" | "right" | "bottom" | "left" => Value::Float(4.0),
            _ => Value::Float(4.0),
        };
        turn.set_prop(view, *key, value);
        shell
            .apply_ops(&turn.commit().unwrap())
            .unwrap_or_else(|err| panic!("{key}: {err}"));
    }

    let mut unknown = Turn::new();
    unknown.set_prop(view, "margin", Value::Float(4.0));
    assert!(shell.apply_ops(&unknown.commit().unwrap()).is_err());
}

#[test]
fn layout_props_apply_before_the_first_layout() {
    use bubble_core::{Turn, Value};

    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 120.0);
    let mut turn = Turn::new();
    let row = turn.spawn(NodeDesc::view(0, 1)).unwrap();
    let mut first = NodeDesc::view(0, row);
    first.width = Some(20.0);
    first.height = Some(20.0);
    let first = turn.spawn(first).unwrap();
    let mut second = NodeDesc::view(0, row);
    second.width = Some(20.0);
    second.height = Some(20.0);
    let second = turn.spawn(second).unwrap();
    let mut absolute = NodeDesc::view(0, row);
    absolute.width = Some(10.0);
    absolute.height = Some(10.0);
    let absolute = turn.spawn(absolute).unwrap();

    turn.set_prop(row, "row", Value::Bool(true));
    turn.set_prop(row, "width", Value::Float(200.0));
    turn.set_prop(row, "height", Value::Float(100.0));
    turn.set_prop(row, "gap", Value::Float(10.0));
    turn.set_prop(row, "alignItems", Value::String("center".into()));
    turn.set_prop(first, "marginLeft", Value::Float(5.0));
    turn.set_prop(absolute, "position", Value::String("absolute".into()));
    turn.set_prop(absolute, "top", Value::Float(12.0));
    turn.set_prop(absolute, "left", Value::Float(7.0));

    shell.apply_ops(&turn.commit().unwrap()).unwrap();
    shell.layout(&mut measure).unwrap();

    let (x1, _, _, _) = shell.node_box(first).unwrap();
    let (x2, _, _, _) = shell.node_box(second).unwrap();
    assert!((x1 - 5.0).abs() < 0.01, "first x {x1}");
    assert!((x2 - 35.0).abs() < 0.01, "second x {x2}");
    let (ax, ay, _, _) = shell.node_box(absolute).unwrap();
    assert!((ax - 7.0).abs() < 0.01, "absolute x {ax}");
    assert!((ay - 12.0).abs() < 0.01, "absolute y {ay}");
}

#[test]
fn text_style_reaches_layout_and_paint() {
    use bubble_core::{Turn, Value};

    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(100.0, 120.0);
    let mut turn = Turn::new();
    let label = turn
        .spawn(NodeDesc::text(0, 1, "A wrapped text node"))
        .unwrap();
    turn.set_prop(label, "lineHeight", Value::Float(30.0));
    turn.set_prop(label, "textAlign", Value::String("center".into()));
    turn.set_prop(label, "fontWeight", Value::Int(700));
    shell.apply_ops(&turn.commit().unwrap()).unwrap();
    shell
        .layout(&mut |text, font, line_height, _weight, max| {
            let width = text.chars().count() as f32 * font * 0.5;
            let width = max.map(|max| width.min(max)).unwrap_or(width);
            let line = if line_height > 0.0 {
                line_height
            } else {
                font * 1.2
            };
            (width, line)
        })
        .unwrap();

    let (_, _, _, h) = shell.node_box(label).unwrap();
    assert!((h - 30.0).abs() < 0.01, "height {h}");
    let paint = shell.paint();
    assert!(paint.iter().any(|cmd| matches!(
        cmd,
        bubble_core::PaintCmd::Text {
            line_height,
            align: 1,
            weight: 700,
            ..
        } if (*line_height - 30.0).abs() < f32::EPSILON
    )));
}

#[test]
fn set_prop_updates_padding_edges() {
    use bubble_core::{Turn, Value};

    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 120.0);
    let mut turn = Turn::new();
    let view = turn.spawn(NodeDesc::view(0, 1)).unwrap();
    let label = turn.spawn(NodeDesc::text(0, view, "Hi")).unwrap();
    shell.apply_ops(&turn.commit().unwrap()).unwrap();
    shell.layout(&mut measure).unwrap();
    let (base_x, base_y, _, _) = shell.node_box(label).unwrap();

    for (key, value) in [
        ("paddingLeft", 11.0),
        ("paddingTop", 7.0),
        ("paddingRight", 5.0),
        ("paddingBottom", 3.0),
    ] {
        turn.set_prop(view, key, Value::Float(value));
    }
    shell.apply_ops(&turn.commit().unwrap()).unwrap();
    shell.layout(&mut measure).unwrap();

    let (x, y, _, _) = shell.node_box(label).unwrap();
    assert!((x - base_x - 11.0).abs() < 0.01, "{base_x} -> {x}");
    assert!((y - base_y - 7.0).abs() < 0.01, "{base_y} -> {y}");
}

#[test]
fn image_node_emits_image_paint() {
    use bubble_core::{Kind, Turn, Value};

    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(100.0, 100.0);
    let mut turn = Turn::new();
    let mut desc = NodeDesc::view(0, 1);
    desc.kind = Kind::Image;
    desc.width = Some(40.0);
    desc.height = Some(30.0);
    let image = turn.spawn(desc).unwrap();
    turn.set_prop(image, "src", Value::String("logo.png".into()));
    turn.set_prop(image, "fit", Value::String("cover".into()));
    shell.apply_ops(&turn.commit().unwrap()).unwrap();
    shell.layout(&mut measure).unwrap();

    let paint = shell.paint();
    assert!(paint.iter().any(|cmd| matches!(
        cmd,
        bubble_core::PaintCmd::Image { src, fit: 1, w, h, .. }
            if src == "logo.png" && (*w - 40.0).abs() < f32::EPSILON && (*h - 30.0).abs() < f32::EPSILON
    )));
}

#[test]
fn unknown_signals_fail() {
    use bubble_core::Turn;

    struct Host;
    impl bubble_core::Sched for Host {
        fn run(&mut self, _: u32) -> i32 {
            0
        }
        fn cleanup(&mut self, _: u32) {}
    }
    let mut turn = Turn::new();
    let mut host = Host;
    assert!(turn.read(0, &mut host).is_err());
    assert!(turn.notify(0, &mut host).is_err());
    let id = turn.alloc_signal();
    turn.read(id, &mut host).unwrap();
    turn.notify(id, &mut host).unwrap();
}

#[test]
fn node_descriptor_and_spawn_op_roundtrip() {
    let mut node = NodeDesc::text(9, 4, "hello");
    node.scroll_x = true;
    node.scroll_y = true;
    node.row = true;
    node.background = 0x1122_3344;
    node.color = 0xffff_ffff;
    node.font_size = 18.0;
    node.radius = 3.0;
    node.handler = 7;
    node.width = Some(120.0);
    node.height = None;
    node.flex_grow = 1.0;
    node.flex_shrink = 0.5;
    node.padding = [1.0, 2.0, 3.0, 4.0];
    node.extensions.push(bubble_core::Extension {
        tag: 7,
        data: vec![1, 2, 3],
    });

    let bytes = node.encode();
    assert_eq!(NodeDesc::decode(&bytes).unwrap(), node);
    assert!(NodeDesc::decode(&bytes[..bytes.len() - 1]).is_err());
    let mut unknown_flag = bytes.clone();
    unknown_flag[15] = 0b100;
    assert!(NodeDesc::decode(&unknown_flag).is_err());

    let batch = encode(12, &[Op::Spawn { desc: node.clone() }]);
    let (seq, ops) = bubble_core::decode_ops(&batch).unwrap();
    assert_eq!(seq, 12);
    assert_eq!(ops, vec![Op::Spawn { desc: node }]);
}

#[test]
fn indexed_spawn_move_and_dispose_reorder_paint() {
    use bubble_core::Turn;

    fn texts(shell: &Shell) -> Vec<String> {
        shell
            .paint()
            .iter()
            .filter_map(|cmd| match cmd {
                bubble_core::PaintCmd::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(100.0, 100.0);
    let mut turn = Turn::new();
    let a = turn.spawn(NodeDesc::text(0, 1, "A")).unwrap();
    let b = turn.spawn(NodeDesc::text(0, 1, "B")).unwrap();
    shell.apply_ops(&turn.commit().unwrap()).unwrap();
    assert_eq!(texts(&shell), vec!["A", "B"]);

    let c = turn.spawn_at(NodeDesc::text(0, 1, "C"), 0).unwrap();
    shell.apply_ops(&turn.commit().unwrap()).unwrap();
    assert_eq!(texts(&shell), vec!["C", "A", "B"]);

    turn.move_node(c, 1, 2);
    shell.apply_ops(&turn.commit().unwrap()).unwrap();
    assert_eq!(texts(&shell), vec!["A", "B", "C"]);

    turn.dispose(a);
    shell.apply_ops(&turn.commit().unwrap()).unwrap();
    assert_eq!(texts(&shell), vec!["B", "C"]);
    assert!(b < c);
}

#[test]
fn generic_set_prop_updates_style_and_paint() {
    use bubble_core::{Turn, Value};

    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 120.0);
    let mut turn = Turn::new();
    let view = turn.spawn(NodeDesc::view(0, 1)).unwrap();
    let label = turn.spawn(NodeDesc::text(0, view, "Hi")).unwrap();
    shell.apply_ops(&turn.commit().unwrap()).unwrap();
    shell.layout(&mut measure).unwrap();

    turn.set_prop(view, "backgroundColor", Value::Int(0xff00_0000));
    turn.set_prop(view, "width", Value::Float(50.0));
    turn.set_prop(label, "color", Value::Int(0xffff_0000));
    turn.set_prop(label, "fontSize", Value::Float(20.0));
    turn.set_prop(view, "opacity", Value::Float(0.5));
    let batch = turn.commit().unwrap();
    let (_, ops) = bubble_core::decode_ops(&batch).unwrap();
    assert_eq!(ops.len(), 5);
    shell.apply_ops(&batch).unwrap();
    shell.layout(&mut measure).unwrap();

    assert_eq!(shell.node_box(view).unwrap().2, 50.0);
    let paint = shell.paint();
    assert!(paint.iter().any(|cmd| matches!(
        cmd,
        bubble_core::PaintCmd::Fill {
            color: 0xff00_0000,
            ..
        }
    )));
    assert!(paint.iter().any(|cmd| matches!(cmd, bubble_core::PaintCmd::Text { color: 0xffff_0000, font_size, .. } if (*font_size - 20.0).abs() < f32::EPSILON)));
    assert!(paint.iter().any(|cmd| matches!(cmd, bubble_core::PaintCmd::PushOpacity { opacity } if (*opacity - 0.5).abs() < f32::EPSILON)));
    assert!(paint
        .iter()
        .any(|cmd| matches!(cmd, bubble_core::PaintCmd::PopOpacity)));
    drop(paint);

    turn.push(Op::SetProp {
        handle: view,
        key: "nope".into(),
        value: Value::Null,
    });
    assert!(shell.apply_ops(&turn.commit().unwrap()).is_err());
}
