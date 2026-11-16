use bubble_core::{encode, Kind, NodeDesc, Op, PaintCmd, Shell, Value};

fn measure(text: &str, font: f32, _: f32, _: u16, _: Option<f32>) -> (f32, f32) {
    (text.chars().count() as f32 * font * 0.5, font)
}

fn layout(shell: &mut Shell) {
    shell.set_viewport(200.0, 200.0);
    shell.layout(&mut measure).unwrap();
}

fn prop(shell: &mut Shell, handle: u32, key: &str, value: Value) {
    shell
        .apply_ops(&encode(
            1,
            &[Op::SetProp {
                handle,
                key: key.into(),
                value,
            }],
        ))
        .unwrap();
}

fn painted(shell: &Shell) -> String {
    shell
        .paint()
        .iter()
        .find_map(|cmd| match cmd {
            PaintCmd::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .expect("text command")
}

#[test]
fn single_line_ellipsis_truncates_on_char_boundaries() {
    let mut shell = Shell::blank().unwrap();
    let mut desc = NodeDesc::text(2, 1, "abcdefghij");
    desc.width = Some(20.0);
    desc.font_size = 10.0;
    desc.flex_shrink = 0.0;
    shell.spawn(desc).unwrap();
    layout(&mut shell);
    assert_eq!(painted(&shell), "abcdefghij");

    prop(
        &mut shell,
        2,
        "textOverflow",
        Value::String("ellipsis".into()),
    );
    layout(&mut shell);
    assert_eq!(painted(&shell), "abc…");

    let mut emoji = NodeDesc::text(3, 1, "a😀bcdef");
    emoji.width = Some(20.0);
    emoji.font_size = 10.0;
    emoji.flex_shrink = 0.0;
    shell.spawn(emoji).unwrap();
    prop(
        &mut shell,
        3,
        "textOverflow",
        Value::String("ellipsis".into()),
    );
    layout(&mut shell);
    let texts: Vec<String> = shell
        .paint()
        .iter()
        .filter_map(|cmd| match cmd {
            PaintCmd::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["abc…", "a😀b…"]);
}

#[test]
fn line_clamp_puts_ellipsis_on_the_last_visible_line() {
    let mut shell = Shell::blank().unwrap();
    let mut desc = NodeDesc::text(2, 1, "abcdefghij");
    desc.width = Some(20.0);
    desc.font_size = 10.0;
    desc.flex_shrink = 0.0;
    shell.spawn(desc).unwrap();
    prop(
        &mut shell,
        2,
        "textOverflow",
        Value::String("ellipsis".into()),
    );
    prop(&mut shell, 2, "lineClamp", Value::Int(2));
    layout(&mut shell);
    assert_eq!(painted(&shell), "abcd\nefg…");
}

#[test]
fn field_ellipsis_is_visual_and_clears_while_focused() {
    let mut shell = Shell::blank().unwrap();
    let mut desc = NodeDesc::view(2, 1);
    desc.kind = Kind::Field;
    desc.text = "abcdefghij".into();
    desc.width = Some(40.0);
    desc.height = Some(20.0);
    desc.font_size = 10.0;
    desc.flex_shrink = 0.0;
    shell.spawn(desc).unwrap();
    prop(
        &mut shell,
        2,
        "textOverflow",
        Value::String("ellipsis".into()),
    );
    layout(&mut shell);
    assert_eq!(painted(&shell), "abcdefg…");
    assert_eq!(shell.text(2).unwrap(), "abcdefghij");

    shell.focus(2).unwrap();
    layout(&mut shell);
    assert_eq!(painted(&shell), "abcdefghij");
}
