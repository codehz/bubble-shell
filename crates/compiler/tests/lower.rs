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
import { signal, mount, Text, View, TapGesture } from "bubble-shell";
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
mount(App);
"##;
    let code = compile("app.tsx", source);
    assert!(code.contains("__bubble_rt_primitive(\"View\""), "{code}");
    assert!(code.contains("__bubble_rt_component(Row"), "{code}");
    assert!(code.contains("__bubble_rt_show"), "{code}");
    // Tags that were only used in JSX are dropped from the author import.
    assert!(
        code.contains("import { signal, mount } from \"bubble-shell\""),
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
        &CompileOptions { source_map: true },
    )
    .unwrap();
    let map = compiled.map.expect("source map requested");
    assert!(map.contains("mapped.tsx"), "{map}");
}

#[test]
fn compiles_the_counter_fixture() {
    let source = include_str!("../../../examples/app/fixtures/counter.tsx");
    let code = compile("counter.tsx", source);
    assert!(code.contains("function Counter()"), "{code}");
    assert!(code.contains("mount(Counter)"), "{code}");
    assert!(code.contains("__bubble_rt_primitive(\"View\""), "{code}");
    assert!(code.contains("__bubble_rt_component(Row"), "{code}");
}
