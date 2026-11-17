use bubble_core::Prop;

#[test]
fn shell_props_parse_to_variants() {
    let schema = bubble_schema::bubble_shell_schema();
    assert_eq!(schema.shell_props().len(), 67);
    for prop in schema.shell_props() {
        let parsed = Prop::parse(prop.key).map(|value| format!("{value:?}"));
        assert_eq!(parsed.as_deref(), Some(prop.variant), "{}", prop.key);
    }
    // JSX shorthands and compiler-only names are expanded or ignored before
    // `Prop::parse`. They must not grow the shell enum.
    for key in [
        "padding",
        "margin",
        "inset",
        "left",
        "right",
        "ref",
        "fontStyle",
        "letterSpacing",
        "onChange",
        "nope",
    ] {
        assert!(Prop::parse(key).is_none(), "{key} is not a shell prop");
    }
}
