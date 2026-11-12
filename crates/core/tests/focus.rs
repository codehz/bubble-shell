use bubble_core::{
    encode, A11yAction, EditCommand, EditMenuAction, Key, Kind, NodeDesc, Op, Role, Shell, Value,
    ACTION_ACTIVATE, ACTION_SCROLL_BACKWARD, ACTION_SCROLL_DOWN, ACTION_SCROLL_FORWARD,
    ACTION_SCROLL_LEFT, ACTION_SCROLL_RIGHT, ACTION_SCROLL_UP, ACTION_SET_VALUE,
};

fn field(shell: &mut Shell, text: &str) -> u32 {
    field_in(shell, 1, text)
}

fn field_in(shell: &mut Shell, parent: u32, text: &str) -> u32 {
    let mut desc = NodeDesc::view(0, parent);
    desc.kind = Kind::Field;
    desc.text = text.into();
    desc.width = Some(200.0);
    desc.height = Some(32.0);
    let handle = shell.next_handle();
    desc.id = handle;
    shell.spawn(desc).unwrap();
    handle
}

fn view(shell: &mut Shell, width: f32, height: f32) -> u32 {
    let mut desc = NodeDesc::view(0, 1);
    desc.width = Some(width);
    desc.height = Some(height);
    let handle = shell.next_handle();
    desc.id = handle;
    shell.spawn(desc).unwrap();
    handle
}

fn set(shell: &mut Shell, seq: u32, handle: u32, key: &str, value: Value) {
    shell
        .apply_ops(&encode(
            seq,
            &[Op::SetProp {
                handle,
                key: key.into(),
                value,
            }],
        ))
        .unwrap();
}

fn layout(shell: &mut Shell) {
    shell.set_viewport(320.0, 480.0);
    shell
        .layout(&mut |text, font_size, line_height, _weight, max_width| {
            let width = text.chars().count() as f32 * font_size * 0.5;
            let width = max_width.map_or(width, |max| width.min(max));
            let line = if line_height > 0.0 {
                line_height
            } else {
                font_size * 1.2
            };
            (width, line)
        })
        .unwrap();
}

#[test]
fn tab_order_skips_hidden_and_disabled_nodes() {
    let mut shell = Shell::blank().unwrap();
    let first = field(&mut shell, "one");
    let button = view(&mut shell, 80.0, 32.0);
    set(&mut shell, 1, button, "focusable", Value::Bool(true));
    set(
        &mut shell,
        2,
        button,
        "role",
        Value::String("button".into()),
    );
    let hidden = field(&mut shell, "hidden");
    set(&mut shell, 3, hidden, "hidden", Value::Bool(true));
    let disabled = field(&mut shell, "disabled");
    set(&mut shell, 4, disabled, "disabled", Value::Bool(true));
    let last = field(&mut shell, "last");

    assert!(shell.focus_next(true));
    assert_eq!(shell.focused(), first);
    assert!(shell.focus_next(true));
    assert_eq!(shell.focused(), button);
    assert!(shell.focus_next(true));
    assert_eq!(
        shell.focused(),
        last,
        "hidden and disabled nodes are skipped"
    );
    assert!(shell.focus_next(true));
    assert_eq!(shell.focused(), first, "tab order wraps");
    assert!(shell.focus_next(false));
    assert_eq!(shell.focused(), last, "shift-tab wraps backwards");

    assert!(shell.key(Key::Escape, 0.0));
    assert_eq!(shell.focused(), 0);
    assert!(!shell.key(Key::Tab, 0.0) || shell.focused() == first);
}

#[test]
fn modal_traps_tab_order_and_hides_background_semantics() {
    let mut shell = Shell::blank().unwrap();
    let outside = field(&mut shell, "outside");
    let dialog = view(&mut shell, 240.0, 120.0);
    set(&mut shell, 1, dialog, "modal", Value::Bool(true));
    set(
        &mut shell,
        2,
        dialog,
        "role",
        Value::String("dialog".into()),
    );
    let inside = field_in(&mut shell, dialog, "inside");

    assert_eq!(shell.active_modal(), dialog);
    let snapshot = shell.semantics();
    let outside_node = snapshot
        .nodes
        .iter()
        .find(|node| node.handle == outside)
        .unwrap();
    let inside_node = snapshot
        .nodes
        .iter()
        .find(|node| node.handle == inside)
        .unwrap();
    assert!(outside_node.hidden);
    assert!(!inside_node.hidden);
    assert_eq!(snapshot.modal, dialog);

    assert!(shell.focus_next(true));
    assert_eq!(shell.focused(), inside, "background field is not reachable");
    assert!(shell.focus_next(true));
    assert_eq!(
        shell.focused(),
        inside,
        "modal order wraps inside the dialog"
    );
}

#[test]
fn modal_rejects_background_focus_and_actions_without_changing_the_field() {
    let mut shell = Shell::blank().unwrap();
    let outside = field(&mut shell, "outside");
    let dialog = view(&mut shell, 240.0, 120.0);
    let inside = field_in(&mut shell, dialog, "inside");
    shell.focus(outside).unwrap();
    set(&mut shell, 1, dialog, "modal", Value::Bool(true));
    assert_eq!(
        shell.focused(),
        0,
        "opening a modal clears background focus"
    );
    shell.focus(inside).unwrap();
    shell.begin_edit_batch().unwrap();
    shell.edit(EditCommand::Insert("pending".into())).unwrap();

    assert!(shell.focus(outside).is_err());
    for action in [
        A11yAction::Focus,
        A11yAction::Activate,
        A11yAction::SetValue("changed".into()),
        A11yAction::ScrollForward,
    ] {
        assert!(shell.a11y_action(outside, action).is_err());
    }
    shell.end_edit_batch().unwrap();
    assert!(shell.field(inside).unwrap().0.contains("pending"));
    assert_eq!(shell.focused(), inside);
    assert_eq!(shell.field(outside).unwrap().0, "outside");
    let snapshot = shell.semantics();
    let background = snapshot
        .nodes
        .iter()
        .find(|node| node.handle == outside)
        .unwrap();
    assert_eq!(background.actions, 0);
    let modal = snapshot
        .nodes
        .iter()
        .find(|node| node.handle == dialog)
        .unwrap();
    assert_eq!((modal.parent, modal.depth), (0, 0));
}

#[test]
fn hidden_modals_do_not_trap_focus_and_nested_modals_are_semantic_roots() {
    let mut shell = Shell::blank().unwrap();
    let outside = field(&mut shell, "outside");
    let outer = view(&mut shell, 240.0, 120.0);
    set(&mut shell, 1, outer, "modal", Value::Bool(true));
    let inner = shell.next_handle();
    shell.spawn(NodeDesc::view(inner, outer)).unwrap();
    set(&mut shell, 2, inner, "modal", Value::Bool(true));
    let field = field_in(&mut shell, inner, "inside");
    let sibling = view(&mut shell, 240.0, 120.0);
    set(&mut shell, 3, sibling, "modal", Value::Bool(true));
    assert_eq!(
        shell.active_modal(),
        inner,
        "the deepest visible modal wins"
    );
    let snapshot = shell.semantics();
    let modal = snapshot
        .nodes
        .iter()
        .find(|node| node.handle == inner)
        .unwrap();
    assert_eq!((modal.parent, modal.depth, modal.hidden), (0, 0, false));
    assert!(shell.focus_next(false));
    assert_eq!(shell.focused(), field);

    set(&mut shell, 4, outer, "hidden", Value::Bool(true));
    assert_eq!(shell.focused(), 0);
    assert_eq!(shell.active_modal(), sibling);
    set(&mut shell, 5, sibling, "hidden", Value::Bool(true));
    assert_eq!(shell.active_modal(), 0);
    assert!(shell.focus_next(true));
    assert_eq!(shell.focused(), outside);
    assert!(
        shell.focus(field).is_err(),
        "hidden ancestors block direct focus"
    );
}

#[test]
fn disabling_a_focused_field_prevents_keyboard_edits_and_pointer_focus() {
    let mut shell = Shell::blank().unwrap();
    let field = field(&mut shell, "line");
    set(&mut shell, 1, field, "multiline", Value::Bool(true));
    shell.focus(field).unwrap();
    layout(&mut shell);
    let bounds = shell
        .semantics()
        .nodes
        .into_iter()
        .find(|node| node.handle == field)
        .unwrap()
        .bounds;
    set(&mut shell, 2, field, "disabled", Value::Bool(true));
    assert_eq!(shell.focused(), 0);
    assert!(!shell.key(Key::Enter, 0.0));
    assert_eq!(shell.field_at(bounds[0] + 1.0, bounds[1] + 1.0), 0);
    assert!(shell.focus(field).is_err());
    assert_eq!(shell.field(field).unwrap().0, "line");
}

#[test]
fn semantics_report_roles_values_actions_and_secure_fields() {
    let mut shell = Shell::blank().unwrap();
    let name = field(&mut shell, "Ada");
    set(&mut shell, 1, name, "label", Value::String("Name".into()));
    let secret = field(&mut shell, "hunter2");
    set(
        &mut shell,
        2,
        secret,
        "label",
        Value::String("Password".into()),
    );
    set(&mut shell, 3, secret, "secure", Value::Bool(true));
    let heading = shell.next_handle();
    let mut desc = NodeDesc::text(0, 1, "Welcome");
    desc.id = heading;
    shell.spawn(desc).unwrap();
    set(
        &mut shell,
        4,
        heading,
        "role",
        Value::String("heading".into()),
    );

    let snapshot = shell.semantics();
    let name = snapshot
        .nodes
        .iter()
        .find(|node| node.handle == name)
        .unwrap();
    assert_eq!(name.role, Role::Textbox);
    assert_eq!(name.label, "Name");
    assert_eq!(name.value, "Ada");
    assert!(name.editable);
    assert_eq!(name.actions & ACTION_SET_VALUE, ACTION_SET_VALUE);
    assert_eq!(name.actions & ACTION_ACTIVATE, 0);

    let secret = snapshot
        .nodes
        .iter()
        .find(|node| node.handle == secret)
        .unwrap();
    assert!(secret.secure);
    assert_eq!(secret.value, "", "secure text never reaches semantics");

    let heading = snapshot
        .nodes
        .iter()
        .find(|node| node.handle == heading)
        .unwrap();
    assert_eq!(heading.role, Role::Heading);
    assert_eq!(heading.label, "Welcome");
}

#[test]
fn enter_activates_a_tap_declaration_and_a11y_actions_route_to_the_field() {
    let mut shell = Shell::blank().unwrap();
    let button = view(&mut shell, 120.0, 40.0);
    set(&mut shell, 1, button, "focusable", Value::Bool(true));
    set(
        &mut shell,
        2,
        button,
        "role",
        Value::String("button".into()),
    );
    shell
        .apply_ops(&encode(
            3,
            &[Op::SpawnGesture {
                gesture: 1,
                target: button,
                kind: 1,
                index: 0,
            }],
        ))
        .unwrap();
    shell
        .apply_ops(&encode(
            4,
            &[Op::BindGestureEvent {
                gesture: 1,
                slot: 4,
                handler: 77,
            }],
        ))
        .unwrap();

    shell.focus(button).unwrap();
    assert!(shell.key(Key::Enter, 10.0));
    let events = shell.take_gesture_events();
    assert_eq!(events.len(), 1);
    assert_eq!(
        (events[0].slot, events[0].handler, events[0].target),
        (4, 77, button)
    );

    let input = field(&mut shell, "old");
    shell.focus(input).unwrap();
    shell
        .a11y_action(input, A11yAction::SetValue("new value".into()))
        .unwrap();
    let (value, state) = shell.field(input).unwrap();
    assert_eq!(value, "new value");
    assert_eq!(state.active(), 9);
    assert!(!shell.take_edit_events().is_empty());
}

#[test]
fn caret_keys_move_by_scalar_and_lines() {
    let mut shell = Shell::blank().unwrap();
    let handle = field(&mut shell, "a😀\nb");
    shell.focus(handle).unwrap();
    shell
        .edit(EditCommand::SetSelection { start: 0, end: 0 })
        .unwrap();

    assert!(shell.key(Key::Right, 0.0));
    assert_eq!(shell.field(handle).unwrap().1.active(), 1);
    assert!(shell.key(Key::Right, 0.0));
    assert_eq!(
        shell.field(handle).unwrap().1.active(),
        3,
        "surrogate is one step"
    );
    assert!(shell.key(Key::End, 0.0));
    assert_eq!(shell.field(handle).unwrap().1.active(), 3);
    assert!(shell.key(Key::Right, 0.0));
    assert_eq!(shell.field(handle).unwrap().1.active(), 4);
    assert!(shell.key(Key::Home, 0.0));
    assert_eq!(
        shell.field(handle).unwrap().1.active(),
        4,
        "line start after newline"
    );
    assert!(shell.key(Key::Left, 0.0));
    assert_eq!(shell.field(handle).unwrap().1.active(), 3);
}

#[test]
fn selection_handles_and_edit_menu_use_the_same_document() {
    let mut shell = Shell::blank().unwrap();
    let handle = field(&mut shell, "abc");
    shell.focus(handle).unwrap();
    shell
        .edit(EditCommand::SetSelection { start: 1, end: 3 })
        .unwrap();
    layout(&mut shell);
    let handles = shell.selection_handles().expect("range has handles");
    assert!(handles[0].0 <= handles[1].0);

    let copied = shell.edit_menu(EditMenuAction::Copy, "").unwrap();
    assert_eq!(copied.clipboard.as_deref(), Some("bc"));

    let cut = shell.edit_menu(EditMenuAction::Cut, "").unwrap();
    assert_eq!(cut.clipboard.as_deref(), Some("bc"));
    assert!(cut.changed);
    assert_eq!(shell.field(handle).unwrap().0, "a");

    shell.edit_menu(EditMenuAction::Paste, "Z").unwrap();
    assert_eq!(shell.field(handle).unwrap().0, "aZ");
    shell.edit_menu(EditMenuAction::SelectAll, "").unwrap();
    let (value, state) = shell.field(handle).unwrap();
    assert_eq!(value, "aZ");
    assert_eq!((state.anchor(), state.active()), (0, 2));

    let secure = field(&mut shell, "secret");
    set(&mut shell, 1, secure, "secure", Value::Bool(true));
    shell.focus(secure).unwrap();
    shell
        .edit(EditCommand::SetSelection { start: 0, end: 6 })
        .unwrap();
    let copied = shell.edit_menu(EditMenuAction::Copy, "").unwrap();
    assert_eq!(copied.clipboard, None, "secure fields never copy");
}

fn scroll_view(shell: &mut Shell, axis_x: bool, axis_y: bool, content: [f32; 2]) -> (u32, u32) {
    let mut scroller = NodeDesc::view(2, 1);
    scroller.scroll_x = axis_x;
    scroller.scroll_y = axis_y;
    scroller.width = Some(100.0);
    scroller.height = Some(100.0);
    let mut inner = NodeDesc::view(3, 2);
    inner.width = Some(content[0]);
    inner.height = Some(content[1]);
    inner.flex_shrink = 0.0;
    let child = 4;
    let mut leaf = NodeDesc::view(child, 3);
    leaf.width = Some(20.0);
    leaf.height = Some(20.0);
    leaf.flex_shrink = 0.0;
    shell
        .apply_ops(&encode(
            1,
            &[
                Op::Spawn { desc: scroller },
                Op::Spawn { desc: inner },
                Op::Spawn { desc: leaf },
            ],
        ))
        .unwrap();
    layout(shell);
    (2, child)
}

#[test]
fn a11y_scroll_actions_move_the_nearest_vertical_container() {
    let mut shell = Shell::blank().unwrap();
    let (scroller, leaf) = scroll_view(&mut shell, false, true, [100.0, 300.0]);
    let snapshot = shell.semantics();
    let node = snapshot
        .nodes
        .iter()
        .find(|node| node.handle == scroller)
        .unwrap();
    assert!(node.scrollable);
    assert_eq!(
        node.actions
            & (ACTION_SCROLL_FORWARD
                | ACTION_SCROLL_BACKWARD
                | ACTION_SCROLL_UP
                | ACTION_SCROLL_DOWN),
        ACTION_SCROLL_FORWARD | ACTION_SCROLL_BACKWARD | ACTION_SCROLL_UP | ACTION_SCROLL_DOWN
    );
    assert_eq!(
        node.actions & (ACTION_SCROLL_LEFT | ACTION_SCROLL_RIGHT),
        0,
        "vertical containers expose no horizontal actions"
    );

    shell.a11y_action(leaf, A11yAction::ScrollForward).unwrap();
    assert_eq!(shell.scroll_of(scroller).unwrap().1, 80.0);
    shell.a11y_action(leaf, A11yAction::ScrollDown).unwrap();
    assert_eq!(shell.scroll_of(scroller).unwrap().1, 160.0);
    shell.a11y_action(leaf, A11yAction::ScrollDown).unwrap();
    assert_eq!(shell.scroll_of(scroller).unwrap().1, 200.0, "clamped");
    shell.a11y_action(leaf, A11yAction::ScrollUp).unwrap();
    assert_eq!(shell.scroll_of(scroller).unwrap().1, 120.0);
    shell.a11y_action(leaf, A11yAction::ScrollBackward).unwrap();
    assert_eq!(shell.scroll_of(scroller).unwrap().1, 40.0);
    assert!(
        shell.a11y_action(leaf, A11yAction::ScrollLeft).is_err(),
        "horizontal action needs a horizontal axis"
    );
    assert_eq!(
        shell.scroll_of(scroller).unwrap(),
        (0.0, 40.0),
        "failed action does not move the container"
    );
}

#[test]
fn modal_scroll_cannot_reach_a_background_scroll_container() {
    let mut shell = Shell::blank().unwrap();
    let (scroller, leaf) = scroll_view(&mut shell, false, true, [100.0, 300.0]);
    set(&mut shell, 2, leaf, "modal", Value::Bool(true));
    assert!(shell.a11y_action(leaf, A11yAction::ScrollDown).is_err());
    assert_eq!(shell.scroll_of(scroller).unwrap(), (0.0, 0.0));
}

#[test]
fn a11y_scroll_primary_axis_follows_vertical_then_horizontal() {
    let mut shell = Shell::blank().unwrap();
    let (horizontal, leaf) = scroll_view(&mut shell, true, false, [300.0, 100.0]);
    shell.a11y_action(leaf, A11yAction::ScrollForward).unwrap();
    assert_eq!(shell.scroll_of(horizontal).unwrap(), (80.0, 0.0));
    shell.a11y_action(leaf, A11yAction::ScrollRight).unwrap();
    assert_eq!(shell.scroll_of(horizontal).unwrap(), (160.0, 0.0));
    shell.a11y_action(leaf, A11yAction::ScrollBackward).unwrap();
    assert_eq!(shell.scroll_of(horizontal).unwrap(), (80.0, 0.0));
    assert!(shell.a11y_action(leaf, A11yAction::ScrollUp).is_err());

    let mut shell = Shell::blank().unwrap();
    let (both, leaf) = scroll_view(&mut shell, true, true, [300.0, 300.0]);
    shell.a11y_action(leaf, A11yAction::ScrollForward).unwrap();
    assert_eq!(
        shell.scroll_of(both).unwrap(),
        (0.0, 80.0),
        "primary axis is vertical when both axes scroll"
    );
    shell.a11y_action(leaf, A11yAction::ScrollRight).unwrap();
    assert_eq!(shell.scroll_of(both).unwrap(), (80.0, 80.0));
    shell.a11y_action(leaf, A11yAction::ScrollDown).unwrap();
    assert_eq!(shell.scroll_of(both).unwrap(), (80.0, 160.0));
}

#[test]
fn a11y_scroll_requires_a_scrollable_ancestor() {
    let mut shell = Shell::blank().unwrap();
    let field = field(&mut shell, "plain");
    assert!(shell.a11y_action(field, A11yAction::ScrollForward).is_err());
}
