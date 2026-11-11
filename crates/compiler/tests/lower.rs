use bubble_compiler::compile;

#[test]
fn lowers_a_signal_text_binding() {
    let js = compile(
        r#"
        import { mount, signal, Text, View } from "bubble";
        function Counter() {
          const count = signal(0);
          return (
            <View background={0xff111111} flexGrow={1}>
              <Text fontSize={24}>Count: {count()}</Text>
              <View onPress={() => count.update((c) => c + 1)}>
                <Text>Increment</Text>
              </View>
            </View>
          );
        }
        mount(Counter);
        "#,
    )
    .unwrap();
    assert!(js.contains("const count = signal(0);"));
    assert!(js.contains("() => \"Count: \" + count()"));
    assert!(js.contains("onPress: () => count.update((c) => c + 1)"));
    assert!(js.contains("mount(Counter);"));
    assert!(!js.contains("from \"bubble\""));
}

#[test]
fn lowers_component_props_children_and_conditionals() {
    let js = compile(
        r#"
        import { mount, Text, View } from "bubble";
        function Card(props) {
          return (
            <View background={props.background ?? 0xff000000}>
              <Text>{props.title}</Text>
              {props.children()}
            </View>
          );
        }
        function App() {
          const enabled = true;
          return (
            <Card title={enabled ? "On" : "Off"} background={enabled ? 0xff112233 : 0xff445566}>
              <Text>Body</Text>
            </Card>
          );
        }
        mount(App);
        "#,
    )
    .unwrap();
    assert!(js.contains("function Card(props) {"));
    assert!(js.contains("component(Card, {title: enabled ? \"On\" : \"Off\", background: enabled ? 4279312947 : 4282668390}, () => {"));
    assert!(js.contains("primitive(\"Text\", {}, () => props.title);"));
    assert!(js.contains("props.children();"));
    assert!(js.contains("primitive(\"Text\", {}, \"Body\");"));
}

#[test]
fn lowers_conditional_children_and_each() {
    let js = compile(
        r#"
        import { mount, Text, View } from "bubble";
        function App() {
          const show = true;
          return (
            <View>
              {show && <Text>On</Text>}
              {show ? <Text>Yes</Text> : null}
              {each(() => items(), (item) => item.id, (item) => <Text>{item.name}</Text>)}
            </View>
          );
        }
        mount(App);
        "#,
    )
    .unwrap();
    assert!(js.contains("when(() => (show), () => {"));
    assert!(js.contains("when(() => (show), () => {"));
    assert!(js.contains("each(() => items(), (item) => item.id, (item) => primitive(\"Text\", {}, () => item.name));"));
}
