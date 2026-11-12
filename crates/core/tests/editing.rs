use bubble_core::{
    encode, EditCommand, Kind, NodeDesc, Op, Shell, Value, DEFERRED_PROPS,
};

fn field(shell: &mut Shell, text: &str) -> u32 {
    let mut desc = NodeDesc::view(0, 1);
    desc.kind = Kind::Field;
    desc.text = text.into();
    desc.width = Some(120.0);
    desc.height = Some(32.0);
    let handle = shell.next_handle();
    desc.id = handle;
    shell.spawn(desc).unwrap();
    handle
}

fn focused(shell: &mut Shell, text: &str) -> u32 {
    let handle = field(shell, text);
    shell.focus(handle).unwrap();
    handle
}

#[test]
fn insert_backspace_and_surrogate_round_trip_through_ops() {
    let mut shell = Shell::blank().unwrap();
    let handle = focused(&mut shell, "");
    shell.edit(EditCommand::Insert("a😀b".into())).unwrap();
    let (value, state) = shell.field(handle).unwrap();
    assert_eq!(value, "a😀b");
    assert_eq!(state.sel_start, 4);
    assert_eq!(state.revision, 1);

    shell
        .edit(EditCommand::SetSelection { start: 4, end: 4 })
        .unwrap();
    shell.edit(EditCommand::Backspace).unwrap();
    let (value, state) = shell.field(handle).unwrap();
    assert_eq!(value, "a😀");
    assert_eq!(state.sel_start, 3);

    shell.edit(EditCommand::Backspace).unwrap();
    let (value, state) = shell.field(handle).unwrap();
    assert_eq!(value, "a");
    assert_eq!(state.sel_start, 1);

    let batch = encode(
        1,
        &[Op::SetEditingValue {
            handle,
            epoch: state.revision,
            value: "ok".into(),
            sel_start: 1,
            sel_end: 2,
            cancel: false,
        }],
    );
    shell.apply_ops(&batch).unwrap();
    let (value, state) = shell.field(handle).unwrap();
    assert_eq!(value, "ok");
    assert_eq!((state.sel_start, state.sel_end), (1, 2));
    let stale = state.revision;
    shell
        .apply_ops(&encode(
            2,
            &[Op::SetEditingValue {
                handle,
                epoch: stale - 1,
                value: "no".into(),
                sel_start: 0,
                sel_end: 0,
                cancel: false,
            }],
        ))
        .unwrap();
    assert_eq!(shell.field(handle).unwrap().0, "ok");
}

#[test]
fn composition_blocks_script_until_cancel_and_single_line_strips_newlines() {
    let mut shell = Shell::blank().unwrap();
    let handle = focused(&mut shell, "");
    shell.edit(EditCommand::CompositionStart).unwrap();
    shell
        .edit(EditCommand::CompositionUpdate("中".into()))
        .unwrap();
    let (value, state) = shell.field(handle).unwrap();
    assert_eq!(value, "中");
    assert_eq!(state.composition, Some((0, 1)));
    let seen = state.revision;
    shell
        .apply_ops(&encode(
            1,
            &[Op::SetEditingValue {
                handle,
                epoch: seen,
                value: "x".into(),
                sel_start: 1,
                sel_end: 1,
                cancel: false,
            }],
        ))
        .unwrap();
    assert_eq!(shell.field(handle).unwrap().0, "中");
    shell
        .apply_ops(&encode(
            2,
            &[Op::SetEditingValue {
                handle,
                epoch: seen,
                value: "完".into(),
                sel_start: 1,
                sel_end: 1,
                cancel: true,
            }],
        ))
        .unwrap();
    let (value, state) = shell.field(handle).unwrap();
    assert_eq!(value, "完");
    assert!(state.composition.is_none());
    assert!(!state.composing);

    shell.edit(EditCommand::Insert("a\nb".into())).unwrap();
    assert_eq!(shell.field(handle).unwrap().0, "完ab");
    shell
        .apply_ops(&encode(
            3,
            &[Op::SetProp {
                handle,
                key: "multiline".into(),
                value: Value::Bool(true),
            }],
        ))
        .unwrap();
    shell.edit(EditCommand::Insert("\n".into())).unwrap();
    assert!(shell.field(handle).unwrap().0.ends_with('\n'));
}

#[test]
fn max_length_counts_utf16_and_secure_paint_masks() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 80.0);
    let handle = focused(&mut shell, "");
    shell
        .apply_ops(&encode(
            1,
            &[
                Op::SetProp {
                    handle,
                    key: "maxLength".into(),
                    value: Value::Int(2),
                },
                Op::SetProp {
                    handle,
                    key: "secure".into(),
                    value: Value::Bool(true),
                },
            ],
        ))
        .unwrap();
    shell.edit(EditCommand::Insert("😀x".into())).unwrap();
    assert_eq!(shell.field(handle).unwrap().0, "😀");
    shell.layout(&mut |_, _, _, _, _| (10.0, 16.0)).unwrap();
    let paint = shell.paint();
    let masked = paint.iter().any(|cmd| {
        matches!(cmd, bubble_core::PaintCmd::Text { text, .. } if text == "••")
    });
    assert!(masked);
    assert!(DEFERRED_PROPS.contains(&"maxLength"));
}

#[test]
fn dispose_drops_focus() {
    let mut shell = Shell::blank().unwrap();
    let handle = focused(&mut shell, "a");
    shell
        .apply_ops(&encode(1, &[Op::DisposeNode { handle }]))
        .unwrap();
    assert_eq!(shell.focused(), 0);
    assert!(shell.edit(EditCommand::Insert("z".into())).is_err());
}

#[test]
fn field_at_returns_the_deepest_field() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 80.0);
    let handle = field(&mut shell, "");
    shell
        .layout(&mut |_, _, _, _, _| (10.0, 16.0))
        .unwrap();
    let (x, y, w, h) = shell.node_box(handle).unwrap();
    assert!(w > 0.0 && h > 0.0);
    assert_eq!(shell.field_at(x + 1.0, y + 1.0), handle);
    assert_eq!(shell.field_at(x + w + 8.0, y + h + 8.0), 0);
}
