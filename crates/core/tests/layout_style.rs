use bubble_core::{encode, NodeDesc, Op, Shell, Value};

fn measure(text: &str, font: f32, line_height: f32, _weight: u16, max: Option<f32>) -> (f32, f32) {
    let width = text.chars().count() as f32 * font * 0.5;
    let width = max.map(|max| width.min(max)).unwrap_or(width);
    let line = if line_height > 0.0 { line_height } else { font };
    (width, line)
}

fn layout(shell: &mut Shell) {
    shell.set_viewport(200.0, 200.0);
    shell.layout(&mut measure).unwrap();
}

fn set(shell: &mut Shell, id: u32, key: &str, value: Value) {
    shell
        .apply_ops(&encode(
            1,
            &[Op::SetProp {
                handle: id,
                key: key.into(),
                value,
            }],
        ))
        .unwrap();
}

fn view(shell: &mut Shell, id: u32, parent: u32) {
    shell.spawn(NodeDesc::view(id, parent)).unwrap();
}

#[test]
fn lengths_percent_align_wrap_and_shrink_default() {
    let mut shell = Shell::blank().unwrap();
    view(&mut shell, 2, 1);
    set(&mut shell, 2, "width", Value::Float(100.0));
    set(&mut shell, 2, "height", Value::Float(80.0));
    set(&mut shell, 2, "flexDirection", Value::String("row".into()));
    set(&mut shell, 2, "alignItems", Value::String("flex-start".into()));
    view(&mut shell, 3, 2);
    set(&mut shell, 3, "width", Value::String("50%".into()));
    set(&mut shell, 3, "height", Value::Float(10.0));
    layout(&mut shell);
    assert_eq!(shell.node_box(3).unwrap().2, 50.0, "percent width is half the parent");
    set(&mut shell, 3, "flexShrink", Value::Float(1.0));
    view(&mut shell, 4, 2);
    set(&mut shell, 4, "width", Value::Float(80.0));
    set(&mut shell, 4, "height", Value::Float(10.0));
    set(&mut shell, 4, "alignSelf", Value::String("flex-end".into()));
    layout(&mut shell);
    let (_, y3, _, _) = shell.node_box(3).unwrap();
    let (_, y4, w4, _) = shell.node_box(4).unwrap();
    assert!(w4 > 50.0, "default flexShrink 0 keeps the definite width");
    assert!(y4 > y3, "alignSelf end drops the second item");

    set(&mut shell, 2, "flexWrap", Value::String("wrap".into()));
    set(&mut shell, 2, "columnGap", Value::Float(10.0));
    set(&mut shell, 2, "width", Value::Float(100.0));
    view(&mut shell, 5, 2);
    set(&mut shell, 5, "width", Value::Float(40.0));
    set(&mut shell, 5, "height", Value::Float(10.0));
    layout(&mut shell);
    let (x5, y5, _, _) = shell.node_box(5).unwrap();
    assert_eq!(x5, 0.0);
    assert!(y5 > y3, "wrap places the third item on the next line");

    set(&mut shell, 3, "height", Value::Null);
    set(&mut shell, 3, "paddingTop", Value::String("10%".into()));
    view(&mut shell, 7, 3);
    set(&mut shell, 7, "height", Value::Float(10.0));
    layout(&mut shell);
    let (_, _, _, h_pad) = shell.node_box(3).unwrap();
    assert!(h_pad >= 20.0, "vertical padding percent uses the parent width");

    view(&mut shell, 6, 1);
    set(&mut shell, 6, "width", Value::Float(40.0));
    set(&mut shell, 6, "aspectRatio", Value::Float(2.0));
    layout(&mut shell);
    let (_, _, w6, h6) = shell.node_box(6).unwrap();
    assert_eq!(w6, 40.0);
    assert_eq!(h6, 20.0);

    set(&mut shell, 6, "height", Value::Float(30.0));
    layout(&mut shell);
    assert_eq!(shell.node_box(6).unwrap().3, 30.0, "both sizes ignore aspectRatio");

    set(&mut shell, 2, "flexDirection", Value::String("row-reverse".into()));
    set(&mut shell, 2, "flexWrap", Value::String("nowrap".into()));
    layout(&mut shell);
    let (x3, _, _, _) = shell.node_box(3).unwrap();
    let (x4, _, _, _) = shell.node_box(4).unwrap();
    assert!(x4 < x3, "row-reverse flips the main axis");
}
