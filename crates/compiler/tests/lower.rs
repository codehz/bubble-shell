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
    assert!(!js.contains("BTIR"));
    assert!(!js.contains("from \"bubble\""));
}
