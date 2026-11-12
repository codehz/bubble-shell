use bubble_compiler::compile;

#[test]
fn lowers_a_signal_text_binding() {
    let js = compile(
        r#"
        import { mount, signal, Text, View } from "bubble-shell";
        function Counter() {
          const count = signal(0);
          return (
            <View backgroundColor={0xff111111} flexGrow={1}>
              <Text fontSize={24}>Count: {count()}</Text>
              <TapGesture onTap={() => count.update((c) => c + 1)} />
                <Text>Increment</Text>
              </View>
          );
        }
        mount(Counter);
        "#,
    )
    .unwrap();
    assert!(js.contains("const count = signal(0);"));
    assert!(js.contains("() => \"Count: \" + count()"));
    assert!(js.contains("onTap: () => count.update((c) => c + 1)"));
    assert!(js.contains("mount(Counter);"));
    assert!(!js.contains("from \"bubble-shell\""));
}

#[test]
fn lowers_gesture_virtual_nodes_and_dynamic_config() {
    let js = compile(
        r#"
        import { mount, signal, Text, View, PanGesture, TransformGesture } from "bubble-shell";
        function App() {
          const editing = signal(false);
          const update = () => {};
          return (
            <View>
              <PanGesture axis="x" minDistance={12} enabled={editing()} onPanUpdate={update} direction={["left"]} />
              <TransformGesture mode={["pan", "pinch"]} onTransformUpdate={update} />
              <Text>Body</Text>
            </View>
          );
        }
        mount(App);
        "#,
    )
    .unwrap();
    assert!(js.contains("primitive(\"PanGesture\", {axis: \"x\", minDistance: 12, enabled: dynamic(() => editing()), onPanUpdate: update, direction: [\"left\"]}, () => {"));
    assert!(js.contains("primitive(\"TransformGesture\", {mode: [\"pan\", \"pinch\"], onTransformUpdate: update}, () => {"));
}

#[test]
fn lowers_a_text_field() {
    let js = compile(
        r#"
        import { mount, View, TextField } from "bubble-shell";
        function App() {
          return (
            <View>
              <TextField value="hi" multiline secure maxLength={8} onChange={() => {}} />
            </View>
          );
        }
        mount(App);
        "#,
    )
    .unwrap();
    assert!(js.contains("primitive(\"TextField\", {value: \"hi\", multiline: true, secure: true, maxLength: 8, onChange: () => {"));
}

#[test]
fn lowers_component_props_children_and_conditionals() {
    let js = compile(
        r#"
        import { mount, Text, View } from "bubble-shell";
        function Card(props) {
          return (
            <View backgroundColor={props.background ?? 0xff000000}>
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
        import { mount, signal, Text, View } from "bubble-shell";
        function App() {
          const color = signal(0xff000000);
          const pad = signal(4);
          return (
            <View backgroundColor={color()} padding={[pad(), pad(), pad(), pad()]}>
              <Text color={color()} backgroundColor={0xff000000}>Body</Text>
              <TapGesture onTap={() => color.update((c) => c + 1)} />
            </View>
          );
        }
        mount(App);
        "#,
    )
    .unwrap();
    assert!(js.contains("import { signal, computed, effect, primitive, mount, component, show, forEach, indexEach, switchFlow, dynamic }"));
    assert!(js.contains("dynamic(() => color())"));
    assert!(js.contains("dynamic(() => [pad(), pad(), pad(), pad()])"));
    assert!(js.contains("onTap: () => color.update((c) => c + 1)"));
    assert!(js.contains("backgroundColor: 4278190080"));
    assert!(!js.contains("dynamic(() => 4278190080)"));
}

#[test]
fn lowers_image_primitive() {
    let js = compile(
        r#"
        import { mount, Image, View } from "bubble-shell";
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
    assert!(js.contains(
        "primitive(\"Image\", {src: \"logo.png\", width: 40, height: 30, fit: \"cover\"}, "
    ));
}

#[test]
fn lowers_computed_values_as_signal_sources() {
    let js = compile(
        r#"
        import { mount, signal, computed, Text, View } from "bubble-shell";
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
fn lowers_show_for_and_switch() {
    let js = compile(
        r#"
        import { mount, Text, View } from "bubble-shell";
        function App() {
          const show = true;
          const items = [];
          return (
            <View>
              <Show when={show} fallback={<Text>Off</Text>}>
                {(value) => <Text>{value}</Text>}
              </Show>
              <For each={items} by={(item) => item.id}>
                {(item) => <Text>{item().name}</Text>}
              </For>
              <Index each={items}>
                {(item) => <Text>{item()}</Text>}
              </Index>
              <Switch fallback={null}>
                <Match when={show}>
                  {(value) => <Text>{value}</Text>}
                </Match>
              </Switch>
            </View>
          );
        }
        mount(App);
        "#,
    )
    .unwrap();
    assert!(js.contains("show(() => (show), () => {"));
    assert!(js.contains("forEach(() => (items), (item) => item.id, (item) => primitive(\"Text\", {}, () => item().name))"));
    assert!(
        js.contains("indexEach(() => (items), (item) => primitive(\"Text\", {}, () => item()))")
    );
    assert!(js.contains("switchFlow(null, ["));
    assert!(compile("<View>{show ? <Text>Yes</Text> : null}</View>").is_err());
}
