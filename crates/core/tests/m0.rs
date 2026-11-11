use bubble_core::{encode, encode_set_prop_color, encode_set_text, validate_bundle_hashes, Kind, NodeDesc, Op, Shell, Template};

fn measure(text: &str, font_size: f32, max_width: Option<f32>) -> (f32, f32) {
    let line = font_size * 1.2;
    let width = text.chars().count() as f32 * font_size * 0.5;
    match max_width {
        Some(max) if max > 0.0 && width > max => {
            let lines = (width / max).ceil().max(1.0);
            (max, lines * line)
        }
        _ => (width, line),
    }
}

fn demo_template() -> Template {
    let mut root = NodeDesc::view(1, 0);
    root.background = 0xff11_1111;
    root.padding = [12.0, 12.0, 12.0, 12.0];

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

    let mut column = NodeDesc::view(6, 5);
    let mut nodes = vec![root, header, button, label, scroller, column];
    for index in 0..30 {
        let mut row = NodeDesc::text(7 + index, 6, format!("Row {index:02}"));
        row.font_size = 16.0;
        row.color = 0xffdd_dddd;
        row.padding = [0.0, 4.0, 0.0, 4.0];
        nodes.push(row);
    }
    let _ = Kind::Text;
    Template { nodes }
}

fn laid_out() -> Shell {
    let bytes = demo_template().encode();
    let mut shell = Shell::mount(&bytes).unwrap();
    shell.set_viewport(320.0, 480.0);
    shell.layout(&mut measure).unwrap();
    shell
}

#[test]
fn template_and_ops_roundtrip() {
    let bytes = demo_template().encode();
    let decoded = Template::decode(&bytes).unwrap();
    assert_eq!(decoded.nodes.len(), 36);
    assert_eq!(decoded.nodes[1].text, "Count: 0");

    let batch = encode_set_text(7, 2, "Count: 3");
    let mut shell = Shell::mount(&bytes).unwrap();
    assert_eq!(shell.apply_ops(&batch).unwrap(), 7);
    assert_eq!(shell.text(2).unwrap(), "Count: 3");

    let color = encode_set_prop_color(8, 3, "backgroundColor", 0xff00_0000);
    assert_eq!(shell.apply_ops(&color).unwrap(), 8);
    shell.apply_ops(&encode_set_text(9, 3, "nope")).unwrap_err();
}

#[test]
fn rejects_bad_magic_and_dense_ids() {
    assert!(Template::decode(b"NOPE").is_err());
    assert!(Shell::mount(&encode_set_text(1, 1, "x")).is_err());
}

#[test]
fn click_scroll_and_paint() {
    let mut shell = laid_out();
    let (x, y, w, h) = shell.node_box(3).unwrap();
    assert!(w > 0.0 && h > 0.0);
    let (lx, ly, _, lh) = shell.node_box(4).unwrap();
    assert!(ly >= y && ly + lh <= y + h + 1.0, "label {ly} outside button {y} {h}");
    assert!(shell.node_box(2).unwrap().1 < 20.0, "header double-padded");
    assert_eq!(shell.tap(lx + 1.0, ly + lh / 2.0), Some(1));

    let (sx, sy, sw, sh) = shell.node_box(5).unwrap();
    assert!(sh > 0.0 && sh < 480.0);
    let before = shell.node_box(7).unwrap().1;
    assert!(shell.scroll_wheel(sx + sw / 2.0, sy + sh / 2.0, 40.0));
    shell.layout(&mut measure).unwrap();
    let after = shell.node_box(7).unwrap().1;
    assert!(after < before);
    assert!(shell.scroll_of(5).unwrap() > 0.0);

    let paint = shell.paint();
    assert!(paint.iter().any(|cmd| matches!(cmd, bubble_core::PaintCmd::Text { text, .. } if text == "Count: 0")));
    assert!(paint.iter().any(|cmd| matches!(cmd, bubble_core::PaintCmd::PushClip { .. })));
}
#[test]
fn create_prepends_and_refuses_a_retired_handle() {
    let mut shell = laid_out();
    let handle = shell.next_handle();
    let batch = encode(
        2,
        &[
            Op::CreateNode {
                handle,
                template_id: 7,
                parent: 6,
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
    assert!(shell.paint().iter().any(|cmd| matches!(cmd, bubble_core::PaintCmd::Text { text, .. } if text == "Added 1")));
    shell
        .apply_ops(&encode(3, &[Op::DisposeNode { handle }]))
        .unwrap();
    assert!(shell.text(handle).is_err());
    let again = encode(
        4,
        &[Op::CreateNode {
            handle,
            template_id: 7,
            parent: 6,
            index: 0,
        }],
    );
    assert!(shell.apply_ops(&again).is_err());
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
