use bubble_core::{
    encode_edit_events, utf16_len, CompositionChange, DocRange, EditConfig, EditDelta, EditError,
    EditEvent, EditOp, EditSession, Expect, Selection,
};

fn byte_index(value: &str, units: u32) -> usize {
    let mut seen = 0u32;
    for (index, ch) in value.char_indices() {
        if seen >= units {
            return index;
        }
        seen += ch.len_utf16() as u32;
    }
    value.len()
}

fn replay(value: &mut String, delta: &EditDelta) {
    let start = byte_index(value, delta.start);
    let end = byte_index(value, delta.end);
    value.replace_range(start..end, &delta.insert);
}

fn insert(session: &mut EditSession, text: &str) {
    session.apply(EditOp::Insert(text.into()), None).unwrap();
}

#[test]
fn positions_snap_surrogate_boundaries_and_keep_direction() {
    let mut session = EditSession::new("a😀b", EditConfig::default());
    session
        .apply(EditOp::Select(Selection::new(4, 2)), None)
        .unwrap();
    let state = session.field_state();
    assert_eq!(state.anchor(), 4);
    assert_eq!(state.active(), 1, "offset 2 is inside the surrogate pair");
    assert!(!state.selection.is_collapsed());

    session.apply(EditOp::Backspace, None).unwrap();
    assert_eq!(session.value(), "a");
    assert_eq!(session.field_state().active(), 1);

    let mut session = EditSession::new("a😀b", EditConfig::default());
    session
        .apply(
            EditOp::Replace {
                range: DocRange::new(2, 2),
                text: "X".into(),
                selection: None,
                composition: CompositionChange::Keep,
            },
            None,
        )
        .unwrap();
    assert_eq!(session.value(), "aX😀b");
}

#[test]
fn range_replace_sets_an_explicit_directional_cursor() {
    let mut session = EditSession::new("hello", EditConfig::default());
    session
        .apply(
            EditOp::Replace {
                range: DocRange::new(1, 4),
                text: "i".into(),
                selection: Some(Selection::new(2, 0)),
                composition: CompositionChange::Keep,
            },
            None,
        )
        .unwrap();
    assert_eq!(session.value(), "hio");
    let state = session.field_state();
    assert_eq!(state.anchor(), 2);
    assert_eq!(state.active(), 0);
    assert_eq!((state.sel_start(), state.sel_end()), (0, 2));

    session
        .apply(
            EditOp::Replace {
                range: DocRange::new(0, 0),
                text: ">".into(),
                selection: None,
                composition: CompositionChange::Keep,
            },
            None,
        )
        .unwrap();
    assert_eq!(session.value(), ">hio");
    assert_eq!(session.field_state().active(), 1);
}

#[test]
fn composition_range_and_selection_are_independent() {
    let mut session = EditSession::new("", EditConfig::default());
    session.apply(EditOp::BeginComposition, None).unwrap();
    session.composition_update("にほん").unwrap();
    assert_eq!(session.field_state().composition, Some((0, 3)));
    assert_eq!(session.field_state().active(), 3);

    session
        .apply(EditOp::Select(Selection::new(1, 3)), None)
        .unwrap();
    let state = session.field_state();
    assert_eq!(state.composition, Some((0, 3)));
    assert_eq!(state.selection.range(), (1, 3));

    session
        .apply(
            EditOp::Replace {
                range: DocRange::new(1, 2),
                text: "本".into(),
                selection: Some(Selection::collapsed(2)),
                composition: CompositionChange::Set(DocRange::new(0, 2)),
            },
            None,
        )
        .unwrap();
    assert_eq!(session.value(), "に本ん");
    assert_eq!(session.field_state().composition, Some((0, 2)));
    assert_eq!(session.field_state().active(), 2);

    session
        .apply(
            EditOp::EndComposition {
                text: Some("日本".into()),
                selection: Some(Selection::new(1, 1)),
            },
            None,
        )
        .unwrap();
    assert_eq!(session.value(), "日本ん");
    let state = session.field_state();
    assert!(!state.composing);
    assert_eq!(state.composition, None);
    assert_eq!(state.anchor(), 1);

    session.apply(EditOp::Undo, None).unwrap();
    assert_eq!(session.value(), "");
}

#[test]
fn script_writes_report_conflicts_and_reset_invalidates() {
    let mut session = EditSession::new("a", EditConfig::default());
    session.apply(EditOp::BeginComposition, None).unwrap();
    let revision = session.revision();
    assert_eq!(
        session
            .script_reset(revision, "b", Selection::collapsed(1), false)
            .unwrap_err(),
        EditError::CompositionConflict
    );

    let other = EditSession::new("x", EditConfig::default());
    assert!(matches!(
        session.apply(
            EditOp::Insert("z".into()),
            Some(Expect::Session(other.id()))
        ),
        Err(EditError::SessionMismatch { .. })
    ));

    session
        .script_reset(revision, "b", Selection::collapsed(1), true)
        .unwrap();
    assert_eq!(session.value(), "b");
    assert!(!session.field_state().composing);
    let after = session.revision();
    assert!(after > 0);
    assert_eq!(
        session
            .script_reset(after - 1, "c", Selection::collapsed(0), false)
            .unwrap_err(),
        EditError::StaleRevision {
            expected: after - 1,
            actual: session.revision()
        }
    );

    session.apply(EditOp::Undo, None).unwrap();
    assert_eq!(session.value(), "b", "reset clears undo history");

    session.close();
    assert_eq!(
        session.apply(EditOp::Insert("d".into()), None).unwrap_err(),
        EditError::Closed
    );
}

#[test]
fn nested_batches_commit_once_and_errors_roll_back_the_batch() {
    let mut session = EditSession::new("", EditConfig::default());
    insert(&mut session, "a");
    let revision = session.revision();
    session.begin_batch().unwrap();
    let first = session.apply(EditOp::Insert("b".into()), None).unwrap();
    assert!(first.changed);
    assert_eq!(session.value(), "ab");
    assert_eq!(session.revision(), revision, "batch has not committed");

    session.begin_batch().unwrap();
    session.apply(EditOp::Insert("c".into()), None).unwrap();
    let inner = session.end_batch().unwrap();
    assert!(!inner.changed);
    assert_eq!(session.value(), "abc");
    let outer = session.end_batch().unwrap();
    assert!(outer.changed);
    assert_eq!(outer.revision, revision + 1);
    assert_eq!(session.value(), "abc");
    assert_eq!(outer.deltas.len(), 2, "both splices are in the commit");

    session.apply(EditOp::Undo, None).unwrap();
    assert_eq!(session.value(), "a", "one batch is one undo step");

    session.begin_batch().unwrap();
    session.apply(EditOp::Insert("x".into()), None).unwrap();
    assert_eq!(
        session.apply(EditOp::Undo, None).unwrap_err(),
        EditError::BatchActive
    );
    assert_eq!(session.value(), "a", "failed batch discarded its edit");
    assert_eq!(session.end_batch().unwrap_err(), EditError::BatchUnderflow);

    session.begin_batch().unwrap();
    session.apply(EditOp::Insert("y".into()), None).unwrap();
    assert_eq!(
        session
            .apply(
                EditOp::EndComposition {
                    text: Some("z".into()),
                    selection: None,
                },
                None
            )
            .unwrap_err(),
        EditError::CompositionConflict
    );
    assert_eq!(session.value(), "a");
}

#[test]
fn deltas_replay_to_the_final_value() {
    let mut session = EditSession::new("a😀bc", EditConfig::default());
    let mut text = "a😀bc".to_string();
    let mut deltas = Vec::new();

    let outcome = session
        .apply(
            EditOp::Replace {
                range: DocRange::new(1, 3),
                text: "XY".into(),
                selection: None,
                composition: CompositionChange::Keep,
            },
            None,
        )
        .unwrap();
    deltas.extend(outcome.deltas);

    let outcome = session.apply(EditOp::Insert("!".into()), None).unwrap();
    deltas.extend(outcome.deltas);

    let outcome = session.apply(EditOp::Backspace, None).unwrap();
    deltas.extend(outcome.deltas);

    for delta in &deltas {
        replay(&mut text, delta);
    }
    assert_eq!(text, session.value());
    assert_eq!(text, "aXYbc");
}

#[test]
fn single_line_and_max_length_apply_to_transactions() {
    let mut session = EditSession::new(
        "",
        EditConfig {
            multiline: false,
            max_length: 2,
        },
    );
    session.insert("😀x").unwrap();
    assert_eq!(session.value(), "😀");
    session.insert("\n").unwrap();
    assert_eq!(session.value(), "😀");

    let mut session = EditSession::new(
        "",
        EditConfig {
            multiline: true,
            max_length: 0,
        },
    );
    session.insert("\r\na").unwrap();
    assert_eq!(session.value(), "\na");
    assert_eq!(utf16_len(session.value()), 2);

    session.set_config(EditConfig {
        multiline: true,
        max_length: 4,
    });
    session
        .apply(EditOp::Select(Selection::collapsed(0)), None)
        .unwrap();
    session.insert("😀").unwrap();
    assert_eq!(session.value(), "😀\na");
}

#[test]
fn session_identity_is_unique_and_expect_checks_revision() {
    let mut session = EditSession::new("", EditConfig::default());
    let other = EditSession::new("", EditConfig::default());
    assert_ne!(session.id(), other.id());
    assert_ne!(session.id().get(), 0);

    let id = session.id();
    let revision = session.revision();
    session
        .apply(
            EditOp::Insert("a".into()),
            Some(Expect::Revision {
                session: id,
                revision,
            }),
        )
        .unwrap();
    assert!(matches!(
        session.apply(
            EditOp::Insert("b".into()),
            Some(Expect::Revision {
                session: id,
                revision
            })
        ),
        Err(EditError::StaleRevision { .. })
    ));
    assert_eq!(session.value(), "a");
}

#[test]
fn edit_events_encode_v2_deltas_and_final_snapshot() {
    let mut session = EditSession::new("a", EditConfig::default());
    let outcome = session
        .apply(EditOp::Insert("b".into()), None)
        .unwrap();
    let event = EditEvent {
        handle: 7,
        handler: 9,
        session: session.id(),
        revision: outcome.revision,
        deltas: outcome.deltas,
        selection: outcome.snapshot.selection,
        composition: None,
        composing: false,
        value: session.value().to_owned(),
    };
    let bytes = encode_edit_events(&[event]);
    assert_eq!(&bytes[..4], b"BSED");
    assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 2);
    assert_eq!(read_u32(&bytes, 6), 1);

    let offset = 10;
    assert_eq!(read_u32(&bytes, offset), 9);
    assert_eq!(read_u32(&bytes, offset + 4), 7);
    assert_eq!(read_u64(&bytes, offset + 8), session.id().get());
    assert_eq!(read_u64(&bytes, offset + 16), outcome.revision);
    assert_eq!(read_u32(&bytes, offset + 24), 2);
    assert_eq!(read_u32(&bytes, offset + 28), 2);
    assert_eq!(read_u32(&bytes, offset + 32), u32::MAX);
    assert_eq!(read_u32(&bytes, offset + 36), u32::MAX);
    assert_eq!(read_u32(&bytes, offset + 40), 0);
    assert_eq!(read_u32(&bytes, offset + 44), 1);

    let delta = offset + 48;
    assert_eq!(read_u32(&bytes, delta), 1);
    assert_eq!(read_u32(&bytes, delta + 4), 1);
    assert_eq!(read_u32(&bytes, delta + 8), 1);
    assert_eq!(&bytes[delta + 12..delta + 13], b"b");

    let value = delta + 13;
    assert_eq!(read_u32(&bytes, value), 2);
    assert_eq!(&bytes[value + 4..], b"ab");
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
