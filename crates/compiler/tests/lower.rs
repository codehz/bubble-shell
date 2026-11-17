use bubble_compiler::{compile_module, CompileOptions};

fn compile(name: &str, source: &str) -> String {
    compile_module(name, source, &CompileOptions::default())
        .unwrap_or_else(|error| panic!("compile failed:\n{error}"))
        .code
}

#[test]
fn keeps_arbitrary_statements_and_exports() {
    let source = r#"
import { signal, mount } from "bubble-shell";
export const limit = 3;
export function helper(value) { return value * 2; }
const twice = (value) => helper(value);
if (limit > 2) { console.log(twice(limit)); }
export default { limit, twice };
const ticks = signal(0);
ticks.update((n) => n + 1);
"#;
    let code = compile("module.js", source);
    assert!(code.contains("export const limit = 3;"), "{code}");
    assert!(
        code.contains("export function helper(value)"),
        "arbitrary declarations survive:\n{code}"
    );
    assert!(code.contains("if (limit > 2)"), "{code}");
    assert!(code.contains("export default"), "{code}");
    assert!(code.contains("ticks.update"), "{code}");
    // `mount` is unused and dropped; `signal` is still referenced.
    assert!(code.contains("from \"bubble-shell\""), "{code}");
    assert!(code.contains("signal"), "{code}");
    assert!(!code.contains("import { signal, mount }"), "{code}");
    // A module without JSX does not need the runtime module at all.
    assert!(!code.contains("bubble:rt"), "{code}");
}

#[test]
fn lowers_primitives_components_and_control_flow() {
    let source = r##"
import { signal, Text, View, TapGesture } from "bubble-shell";
function Row(props) {
  return <Text color="#ddd">{props.label}</Text>;
}
function App() {
  const open = signal(false);
  return (
    <View gap={8}>
      <Row label="one" />
      <Show when={open()} fallback={<Text>closed</Text>}>
        {() => <Text>open</Text>}
      </Show>
    </View>
  );
}
export default App;
"##;
    let code = compile("app.tsx", source);
    assert!(code.contains("__bubble_rt_primitive(\"View\""), "{code}");
    assert!(
        code.contains("__bubble_rt_component(Row, { label: () => \"one\" })"),
        "component props compile to getters:\n{code}"
    );
    assert!(code.contains("__bubble_rt_show"), "{code}");
    // Tags that were only used in JSX are dropped from the author import.
    assert!(
        code.contains("import { signal } from \"bubble-shell\""),
        "{code}"
    );
    assert!(!code.contains("TapGesture"), "{code}");
}

#[test]
fn wraps_signal_reads_in_dynamic_but_not_handlers() {
    let source = r#"
import { signal, Text, View, TapGesture } from "bubble-shell";
function App() {
  const count = signal(0);
  const label = "Count";
  return (
    <View width={count() + 10} height={20}>
      <TapGesture onTap={() => count.update((c) => c + 1)} />
      <Text>{label}: {count()}</Text>
    </View>
  );
}
"#;
    let code = compile("counter.tsx", source);
    assert!(
        code.contains("__bubble_rt_dynamic(() => count() + 10)"),
        "{code}"
    );
    assert!(
        !code.contains("dynamic(() => () =>"),
        "handlers are not wrapped:\n{code}"
    );
    assert!(code.contains("onTap: () => count.update"), "{code}");
    assert!(code.contains("label + \": \" + count()"), "{code}");
}

#[test]
fn lowers_component_getter_props() {
    let source = r#"
import { Text } from "bubble-shell";
function Row(props) {
  return <Text>{props.label()}</Text>;
}
function App() {
  return <Row label={"n" + 1} flag={true} onTap={() => 1} />;
}
"#;
    let code = compile("app.tsx", source);
    assert!(
        code.contains("label: () => \"n\" + 1"),
        "value props become getters:\n{code}"
    );
    assert!(code.contains("flag: () => true"), "{code}");
    assert!(
        code.contains("onTap: () => 1"),
        "handlers pass through unwrapped:\n{code}"
    );
}

#[test]
fn reports_bare_signal_props() {
    let source = r#"
import { signal, Text } from "bubble-shell";
const live = signal(0);
function Pad(props) {
  return <Text>{props.live()}</Text>;
}
function App() {
  return <Pad live={live} />;
}
"#;
    let error = compile_module("bad.tsx", source, &CompileOptions::default()).unwrap_err();
    assert!(
        error.contains(
            "prop live is a signal binding; pass its current value with a read: live={live()}"
        ),
        "{error}"
    );
}

#[test]
fn lowers_element_bindings_to_content_branches() {
    let source = r#"
import { signal, Text, View } from "bubble-shell";
function App() {
  const open = signal(false);
  const pad = <Text>pad</Text>;
  return (
    <View>
      {pad}
      <View>{open() ? <Text>a</Text> : <Text>b</Text>}</View>
      <View>{() => <Text>thunk</Text>}</View>
    </View>
  );
}
"#;
    let code = compile("app.tsx", source);
    assert!(
        code.contains("const pad = () => __bubble_rt_primitive(\"Text\""),
        "element bindings declare render thunks:\n{code}"
    );
    assert!(
        code.contains("__bubble_rt_content(pad);"),
        "element references become content branches:\n{code}"
    );
    assert!(
        code.contains("__bubble_rt_content(() => {"),
        "compound JSX expressions become content branches:\n{code}"
    );
    assert!(
        code.contains("typeof b === \"function\""),
        "compound branches call the thunk they yield:\n{code}"
    );
    assert!(
        code.contains("__bubble_rt_content(() => __bubble_rt_primitive(\"Text\""),
        "explicit thunks pass through:\n{code}"
    );
}

#[test]
fn keeps_plain_children_as_in_place_text() {
    let source = r#"
import { signal, Text, View } from "bubble-shell";
function App() {
  const count = signal(0);
  return (
    <View>
      {count()}
      {"literal"}
    </View>
  );
}
"#;
    let code = compile("app.tsx", source);
    assert!(
        code.contains("__bubble_rt_primitive(\"Text\", {}, () => String(count()))"),
        "signal-reading calls stay cheap in-place text:\n{code}"
    );
    assert!(
        code.contains("__bubble_rt_primitive(\"Text\", {}, () => String(\"literal\"))"),
        "{code}"
    );
}

#[test]
fn reports_function_calls_in_children() {
    let source = r#"
import { Text, View } from "bubble-shell";
function make() {
  return null;
}
function App() {
  return (
    <View>
      {make()}
    </View>
  );
}
"#;
    let error = compile_module("bad.tsx", source, &CompileOptions::default()).unwrap_err();
    assert!(
        error.contains("component calls must be written as JSX tags or element bindings"),
        "{error}"
    );
}

#[test]
fn reports_element_values_in_text() {
    let source = r#"
import { Text } from "bubble-shell";
function App() {
  const pad = <Text>pad</Text>;
  return <Text>{pad}</Text>;
}
"#;
    let error = compile_module("bad.tsx", source, &CompileOptions::default()).unwrap_err();
    assert!(
        error.contains("Text children must be text or value expressions"),
        "{error}"
    );
}

#[test]
fn does_not_track_signals_across_exports() {
    let source = r#"
import { signal, Text } from "bubble-shell";
export const shared = signal(0);
function App() {
  return <Text>{shared()}</Text>;
}
"#;
    let code = compile("shared.tsx", source);
    // `shared` is exported, so reading it is a plain value read.
    assert!(!code.contains("__bubble_rt_dynamic("), "{code}");
}

#[test]
fn reports_fragments_and_spread_with_a_code_frame() {
    let error = compile_module(
        "bad.tsx",
        "import { View, Text } from \"bubble-shell\";\nfunction App() {\n  return <><Text/></>;\n}\n",
        &CompileOptions::default(),
    )
    .unwrap_err();
    assert!(error.contains("bad.tsx:3:"), "{error}");
    assert!(error.contains("fragment"), "{error}");

    let error = compile_module(
        "spread.tsx",
        "import { View } from \"bubble-shell\";\nfunction App() {\n  return <View {...{}} />;\n}\n",
        &CompileOptions::default(),
    )
    .unwrap_err();
    assert!(error.contains("spread"), "{error}");
}

#[test]
fn rejects_unknown_tags_and_unsupported_extensions() {
    let error = compile_module(
        "unknown.tsx",
        "function App() {\n  return <div>hi</div>;\n}\n",
        &CompileOptions::default(),
    )
    .unwrap_err();
    assert!(error.contains("unknown primitive <div>"), "{error}");

    let error = compile_module("style.css", "", &CompileOptions::default()).unwrap_err();
    assert!(error.contains("unsupported module"), "{error}");
}

#[test]
fn emits_a_source_map_on_request() {
    let compiled = compile_module(
        "mapped.tsx",
        "import { Text } from \"bubble-shell\";\nfunction App() { return <Text>hi</Text>; }\n",
        &CompileOptions {
            source_map: true,
            ..CompileOptions::default()
        },
    )
    .unwrap();
    let map = compiled.map.expect("source map requested");
    assert!(map.contains("mapped.tsx"), "{map}");
}

#[test]
fn compiles_the_counter_fixture() {
    let source = r#"
import { signal, Text, View, TapGesture } from "bubble-shell";
const count = signal(0);
function Row(props) {
  return <Text>{props.label}</Text>;
}
function Counter() {
  return (
    <View>
      <Text>Count: {count()}</Text>
      <View onTap={() => count.update((value) => value + 1)}>
        <Row label="Item 1" />
        <Row label="Item 2" />
      </View>
    </View>
  );
}
export default Counter;
"#;
    let code = compile("counter.tsx", source);
    assert!(code.contains("function Counter()"), "{code}");
    assert!(code.contains("export default Counter"), "{code}");
    assert!(code.contains("__bubble_rt_primitive(\"View\""), "{code}");
    assert!(code.contains("__bubble_rt_component(Row"), "{code}");
}

#[test]
fn hot_mode_wraps_components_signals_and_mount() {
    let source = r#"
import { signal, Text, View } from "bubble-shell";
const shared = signal(2);
const Card = (props) => <Text>{props.label}</Text>;
function App() {
  return (
    <View>
      <Card label="one" />
      <Text>{shared()}</Text>
    </View>
  );
}
export default App;
"#;
    let compiled = compile_module(
        "app.tsx",
        source,
        &CompileOptions {
            source_map: false,
            hot: Some("/repo/app.tsx".to_string()),
        },
    )
    .unwrap();
    let code = compiled.code;
    assert!(code.contains("from \"bubble:hot\""), "{code}");
    assert!(
        code.contains("App = __bubble_hot_component(\"/repo/app.tsx:App\", App)"),
        "{code}"
    );
    assert!(
        code.contains("__bubble_hot_keep(\"/repo/app.tsx:shared\""),
        "top-level signals are kept:\n{code}"
    );
    assert!(
        code.contains("__bubble_hot_component(\"/repo/app.tsx:Card\""),
        "arrow components are wrapped:\n{code}"
    );
    assert!(
        code.contains("__bubble_hot_mount(\"/repo/app.tsx\", App)"),
        "the default export is mounted:\n{code}"
    );
    assert!(code.contains("export default App"), "{code}");
}

#[test]
fn release_mode_has_no_hot_runtime() {
    let source = "import { Text } from \"bubble-shell\";\nfunction App() { return <Text>hi</Text>; }\nexport default App;\n";
    let code = compile("app.tsx", source);
    assert!(!code.contains("bubble:hot"), "{code}");
    assert!(!code.contains("__bubble_hot"), "{code}");
}
