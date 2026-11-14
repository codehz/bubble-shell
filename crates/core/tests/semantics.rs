use bubble_core::{encode, spawn_ops, NodeDesc, Op, Shell, Value};

fn measure(text: &str, font_size: f32) -> (f32, f32) {
    (text.len() as f32 * font_size * 0.5, font_size)
}

fn laid_out() -> Shell {
    let mut shell = Shell::blank().unwrap();
    shell.set_viewport(320.0, 200.0);
    shell
}

#[test]
fn test_id_reaches_the_semantics_snapshot() {
    let mut shell = laid_out();
    let mut text = NodeDesc::text(2, 1, "Count: 0");
    text.handler = 9;
    shell
        .apply_ops(&encode(
            1,
            &[
                spawn_ops(text),
                vec![Op::SetProp {
                    handle: 2,
                    key: "testID".into(),
                    value: Value::String("counter-label".into()),
                }],
            ]
            .concat(),
        ))
        .unwrap();
    shell.layout(&mut |t, f, _, _, _| measure(t, f)).unwrap();
    let snapshot =
        serde_json::from_str::<bubble_core::SemanticsSnapshot>(&shell.semantics_json())
            .expect("snapshot parses");
    assert_eq!(snapshot.nodes.len(), 1);
    assert_eq!(snapshot.nodes[0].test_id, "counter-label");
    assert_eq!(snapshot.nodes[0].label, "Count: 0");
}

#[test]
fn unset_test_id_serializes_empty() {
    let mut shell = laid_out();
    shell
        .apply_ops(&encode(1, &spawn_ops(NodeDesc::text(2, 1, "plain"))))
        .unwrap();
    shell.layout(&mut |t, f, _, _, _| measure(t, f)).unwrap();
    let snapshot =
        serde_json::from_str::<bubble_core::SemanticsSnapshot>(&shell.semantics_json())
            .expect("snapshot parses");
    assert_eq!(snapshot.nodes[0].test_id, "");
}
