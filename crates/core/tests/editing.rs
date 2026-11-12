use bubble_core::{
    encode, EditCommand, EditError, Kind, NodeDesc, Op, Shell, Value, DEFERRED_PROPS,
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
    assert_eq!(state.sel_start(), 4);
    assert_eq!(state.revision, 1);

    shell
        .edit(EditCommand::SetSelection { start: 4, end: 4 })
        .unwrap();
    shell.edit(EditCommand::Backspace).unwrap();
    let (value, state) = shell.field(handle).unwrap();
    assert_eq!(value, "a😀");
    assert_eq!(state.sel_start(), 3);

    shell.edit(EditCommand::Backspace).unwrap();
    let (value, state) = shell.field(handle).unwrap();
    assert_eq!(value, "a");
    assert_eq!(state.sel_start(), 1);

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
    assert_eq!((state.sel_start(), state.sel_end()), (1, 2));
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
fn ime_commit_replaces_the_composition_as_one_history_step() {
    let mut shell = Shell::blank().unwrap();
    let handle = focused(&mut shell, "ab");
    shell.compose_text("に", 1).unwrap();
    let (value, state) = shell.field(handle).unwrap();
    assert_eq!(value, "abに");
    assert_eq!(state.composition, Some((2, 3)));
    assert_eq!((state.sel_start(), state.sel_end()), (3, 3));

    shell
        .edit(EditCommand::SetSelection { start: 2, end: 2 })
        .unwrap();
    shell.commit_text("日本", 1).unwrap();
    let (value, state) = shell.field(handle).unwrap();
    assert_eq!(value, "ab日本");
    assert_eq!((state.sel_start(), state.sel_end()), (4, 4));
    assert!(state.composition.is_none());
    assert!(!state.composing);

    shell.edit(EditCommand::Undo).unwrap();
    assert_eq!(shell.field(handle).unwrap().0, "ab");
    shell.edit(EditCommand::Redo).unwrap();
    assert_eq!(shell.field(handle).unwrap().0, "ab日本");
}

#[test]
fn a_composition_accepts_an_inner_selection_before_commit() {
    let mut shell = Shell::blank().unwrap();
    let handle = focused(&mut shell, "");
    shell.compose_text("abcd", 1).unwrap();
    shell
        .edit(EditCommand::SetSelection { start: 1, end: 2 })
        .unwrap();
    let (value, state) = shell.field(handle).unwrap();
    assert_eq!(value, "abcd");
    assert_eq!((state.sel_start(), state.sel_end()), (1, 2));
    assert_eq!(state.composition, Some((0, 4)));

    shell.commit_text("X", 1).unwrap();
    let (value, state) = shell.field(handle).unwrap();
    assert_eq!(value, "X");
    assert_eq!((state.sel_start(), state.sel_end()), (1, 1));
    assert!(state.composition.is_none());
}

#[test]
fn an_empty_ime_commit_deletes_only_the_selection() {
    let mut shell = Shell::blank().unwrap();
    let handle = focused(&mut shell, "abcd");
    shell
        .edit(EditCommand::SetSelection { start: 1, end: 3 })
        .unwrap();
    shell.commit_text("", 1).unwrap();
    let (value, state) = shell.field(handle).unwrap();
    assert_eq!(value, "ad");
    assert_eq!((state.sel_start(), state.sel_end()), (1, 1));
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
    let masked = paint
        .iter()
        .any(|cmd| matches!(cmd, bubble_core::PaintCmd::Text { text, .. } if text == "••"));
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
    shell.layout(&mut |_, _, _, _, _| (10.0, 16.0)).unwrap();
    let (x, y, w, h) = shell.node_box(handle).unwrap();
    assert!(w > 0.0 && h > 0.0);
    assert_eq!(shell.field_at(x + 1.0, y + 1.0), handle);
    assert_eq!(shell.field_at(x + w + 8.0, y + h + 8.0), 0);
}

#[test]
fn focus_point_places_the_caret_on_the_tapped_line() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 80.0);
    let handle = field(&mut shell, "ab\ncd");
    let mut measure = |text: &str, font: f32, _: f32, _: u16, _: Option<f32>| {
        (text.chars().count() as f32 * 10.0, font)
    };
    shell.layout(&mut measure).unwrap();
    let (x, y, _, _) = shell.node_box(handle).unwrap();
    assert_eq!(
        shell.focus_point(x + 25.0, y + 4.0, &mut measure).unwrap(),
        handle
    );
    assert_eq!(shell.field(handle).unwrap().1.sel_start(), 2);
    assert_eq!(
        shell.focus_point(x + 5.0, y + 20.0, &mut measure).unwrap(),
        handle
    );
    let state = shell.field(handle).unwrap().1;
    assert_eq!((state.sel_start(), state.sel_end()), (3, 3));
    assert_eq!(
        shell
            .focus_point(x + 400.0, y + 400.0, &mut measure)
            .unwrap(),
        0
    );
}

#[test]
fn undo_redo_and_script_reset() {
    let mut shell = Shell::blank().unwrap();
    let handle = focused(&mut shell, "");
    shell.edit(EditCommand::Insert("a".into())).unwrap();
    shell.edit(EditCommand::Insert("b".into())).unwrap();
    shell.edit(EditCommand::Undo).unwrap();
    assert_eq!(shell.field(handle).unwrap().0, "a");
    shell.edit(EditCommand::Redo).unwrap();
    assert_eq!(shell.field(handle).unwrap().0, "ab");
    shell.edit(EditCommand::Undo).unwrap();
    shell.edit(EditCommand::Insert("c".into())).unwrap();
    shell.edit(EditCommand::Redo).unwrap();
    assert_eq!(shell.field(handle).unwrap().0, "ac");

    shell.edit(EditCommand::CompositionStart).unwrap();
    shell
        .edit(EditCommand::CompositionUpdate("中".into()))
        .unwrap();
    shell
        .edit(EditCommand::CompositionEnd("中".into()))
        .unwrap();
    assert_eq!(shell.field(handle).unwrap().0, "ac中");
    shell.edit(EditCommand::Undo).unwrap();
    assert_eq!(shell.field(handle).unwrap().0, "ac");

    let revision = shell.field(handle).unwrap().1.revision;
    shell
        .apply_ops(&encode(
            1,
            &[Op::SetEditingValue {
                handle,
                epoch: revision,
                value: "z".into(),
                sel_start: 1,
                sel_end: 1,
                cancel: false,
            }],
        ))
        .unwrap();
    shell.edit(EditCommand::Undo).unwrap();
    assert_eq!(shell.field(handle).unwrap().0, "z");
}

#[test]
fn unit_at_does_not_move_the_caret() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 80.0);
    let handle = focused(&mut shell, "ab\ncd");
    shell
        .edit(EditCommand::SetSelection { start: 0, end: 0 })
        .unwrap();
    let mut measure = |text: &str, font: f32, _: f32, _: u16, _: Option<f32>| {
        (text.chars().count() as f32 * 10.0, font)
    };
    shell.layout(&mut measure).unwrap();
    let (x, y, _, _) = shell.node_box(handle).unwrap();
    assert_eq!(
        shell.unit_at(x + 25.0, y + 4.0, &mut measure).unwrap(),
        Some(2)
    );
    assert_eq!(shell.field(handle).unwrap().1.sel_start(), 0);
    let (cx, cy, cw, ch) = shell.caret_rect_at(2, &mut measure).unwrap().unwrap();
    assert!(cx >= x + 20.0);
    assert!(cy >= y && ch > 0.0 && cw == 1.0);
    shell.focus(0).unwrap();
    assert_eq!(shell.unit_at(x, y, &mut measure).unwrap(), None);
}

#[test]
fn focused_field_paints_a_caret_and_reports_keyboard_flags() {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(200.0, 80.0);
    let handle = field(&mut shell, "ab");
    shell
        .apply_ops(&encode(
            1,
            &[
                Op::SetProp {
                    handle,
                    key: "paddingLeft".into(),
                    value: Value::Float(8.0),
                },
                Op::SetProp {
                    handle,
                    key: "paddingTop".into(),
                    value: Value::Float(6.0),
                },
                Op::SetProp {
                    handle,
                    key: "secure".into(),
                    value: Value::Bool(true),
                },
                Op::SetProp {
                    handle,
                    key: "keyboard".into(),
                    value: Value::String("email".into()),
                },
                Op::SetProp {
                    handle,
                    key: "autofill".into(),
                    value: Value::String("email".into()),
                },
            ],
        ))
        .unwrap();
    shell.focus(handle).unwrap();
    shell
        .layout(&mut |text, font, _, _, _| (text.chars().count() as f32 * font, font))
        .unwrap();
    assert_eq!(shell.field_flags(handle).unwrap(), 2 | (2 << 8) | (4 << 16));
    let (x, y, w, h) = shell.caret_box().unwrap();
    assert!(w > 0.0 && h > 0.0);
    let (nx, ny, _, _) = shell.node_box(handle).unwrap();
    let paint = shell.paint();
    let text_at = paint.iter().position(|cmd| {
        matches!(
            cmd,
            bubble_core::PaintCmd::Text { x: tx, y: ty, .. }
                if (*tx - (nx + 8.0)).abs() < 0.01 && (*ty - (ny + 6.0)).abs() < 0.01
        )
    });
    let caret_at = paint.iter().position(|cmd| {
        matches!(
            cmd,
            bubble_core::PaintCmd::Fill { x: cx, w: cw, .. } if *cw == 1.0 && (*cx - x).abs() < 0.01
        )
    });
    assert!(text_at.is_some() && caret_at.is_some() && caret_at > text_at);
    let _ = y;
}

#[test]
fn selection_direction_survives_the_shell_boundary() {
    let mut shell = Shell::blank().unwrap();
    let handle = focused(&mut shell, "abcd");
    shell
        .edit(EditCommand::SetSelection { start: 3, end: 1 })
        .unwrap();
    let state = shell.field(handle).unwrap().1;
    assert_eq!(state.anchor(), 3);
    assert_eq!(state.active(), 1);
    assert_eq!((state.sel_start(), state.sel_end()), (1, 3));
    let event = shell.take_edit_events().pop().unwrap();
    assert_eq!(event.selection.anchor.offset, 3);
    assert_eq!(event.selection.active.offset, 1);
    assert!(event.deltas.is_empty());
}

#[test]
fn rejected_script_writes_enter_the_conflict_queue() {
    let mut shell = Shell::blank().unwrap();
    let handle = focused(&mut shell, "");
    shell.edit(EditCommand::CompositionStart).unwrap();
    shell
        .edit(EditCommand::CompositionUpdate("中".into()))
        .unwrap();
    let revision = shell.field(handle).unwrap().1.revision;
    shell
        .apply_ops(&encode(
            1,
            &[Op::SetEditingValue {
                handle,
                epoch: revision,
                value: "x".into(),
                sel_start: 1,
                sel_end: 1,
                cancel: false,
            }],
        ))
        .unwrap();
    assert_eq!(shell.field(handle).unwrap().0, "中");
    let conflicts = shell.take_edit_conflicts();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].handle, handle);
    assert_eq!(conflicts[0].error, EditError::CompositionConflict);

    shell
        .apply_ops(&encode(
            2,
            &[Op::SetEditingValue {
                handle,
                epoch: revision - 1,
                value: "y".into(),
                sel_start: 0,
                sel_end: 0,
                cancel: true,
            }],
        ))
        .unwrap();
    assert_eq!(shell.field(handle).unwrap().0, "中");
    let conflicts = shell.take_edit_conflicts();
    assert_eq!(conflicts.len(), 1);
    assert!(matches!(
        conflicts[0].error,
        EditError::StaleRevision { .. }
    ));
}

#[test]
fn nested_edit_batches_publish_one_event() {
    let mut shell = Shell::blank().unwrap();
    let handle = focused(&mut shell, "ab");
    shell.begin_edit_batch().unwrap();
    shell.edit(EditCommand::Insert("c".into())).unwrap();
    shell.begin_edit_batch().unwrap();
    shell.edit(EditCommand::Insert("d".into())).unwrap();
    assert!(shell.take_edit_events().is_empty());
    shell.end_edit_batch().unwrap();
    assert!(shell.take_edit_events().is_empty());
    shell.end_edit_batch().unwrap();

    let events = shell.take_edit_events();
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert_eq!(event.value, "abcd");
    assert_eq!(event.handle, handle);
    assert_eq!(event.revision, 1);
    assert_eq!(event.deltas.len(), 2);
    assert_eq!(event.selection.active.offset, 4);
    assert_eq!(event.composition, None);
    assert!(!event.composing);
}

#[test]
fn a_failed_batch_command_discards_the_batch() {
    let mut shell = Shell::blank().unwrap();
    let handle = focused(&mut shell, "ab");
    shell.begin_edit_batch().unwrap();
    shell.edit(EditCommand::Insert("x".into())).unwrap();
    assert!(shell.edit(EditCommand::CompositionEnd("".into())).is_err());
    assert_eq!(shell.field(handle).unwrap().0, "ab");
    assert_eq!(shell.end_edit_batch().unwrap_err(), "no edit batch");
    assert!(shell.take_edit_events().is_empty());
}

#[test]
fn focus_loss_discards_an_open_batch() {
    let mut shell = Shell::blank().unwrap();
    let handle = focused(&mut shell, "ab");
    shell.begin_edit_batch().unwrap();
    shell.edit(EditCommand::Insert("x".into())).unwrap();
    shell.focus(0).unwrap();
    assert_eq!(shell.field(handle).unwrap().0, "ab");
    assert_eq!(shell.text(handle).unwrap(), "ab");
    assert!(shell.take_edit_events().is_empty());
    assert_eq!(shell.end_edit_batch().unwrap_err(), "no edit batch");
}
