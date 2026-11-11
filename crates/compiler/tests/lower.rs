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
fn lowers_signal_reading_attributes_to_dynamic_bindings() {
    let js = compile(
        r#"
        import { mount, signal, Text, View } from "bubble";
        function App() {
          const color = signal(0xff000000);
          const pad = signal(4);
          return (
            <View background={color()} padding={[pad(), pad(), pad(), pad()]}>
              <Text color={color()}>Body</Text>
              <View onPress={() => color.update((c) => c + 1)} background={0xff000000} />
            </View>
          );
        }
        mount(App);
        "#,
    )
    .unwrap();
    assert!(js.contains("import { signal, computed, effect, primitive, mount, component, when, each, dynamic }"));
    assert!(js.contains("dynamic(() => color())"));
    assert!(js.contains("dynamic(() => [pad(), pad(), pad(), pad()])"));
    assert!(js.contains("onPress: () => color.update((c) => c + 1)"));
    assert!(js.contains("background: 4278190080"));
    assert!(!js.contains("dynamic(() => 4278190080)"));
}

#[test]
fn lowers_image_primitive() {
    let js = compile(
        r#"
        import { mount, Image, View } from "bubble";
        function App() {
          return (
            <View>
              <Image src="logo.png" width={40} height={30} fit="cover" />
            </View>
          );
        }
        mount(App);
        "#,
    )
    .unwrap();
    assert!(js.contains("primitive(\"Image\", {src: \"logo.png\", width: 40, height: 30, fit: \"cover\"}, "));
}

#[test]
fn lowers_computed_values_as_signal_sources() {
    let js = compile(
        r#"
        import { mount, signal, computed, Text, View } from "bubble";
        function App() {
          const count = signal(1);
          const double = computed(() => count() * 2);
          return (
            <View opacity={double()}>
              <Text>{double()}</Text>
            </View>
          );
        }
        mount(App);
        "#,
    )
    .unwrap();
    assert!(js.contains("const double = computed(() => count() * 2);"));
    assert!(js.contains("dynamic(() => double())"));
    assert!(js.contains("() => double()"));
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
