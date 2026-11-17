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
      <View>
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
            optimize: false,
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

fn errors(name: &str, source: &str) -> String {
    compile_module(name, source, &CompileOptions::default()).unwrap_err()
}

#[test]
fn collects_multiple_errors() {
    let src = r#"
        function App() {
            return (
                <View>
                    <div />
                    <>fragment</>
                    {call()}
                </View>
            );
        }
        export default App;
    "#;
    let err = errors("multi.tsx", src);
    assert_eq!(err.matches("multi.tsx:").count(), 3, "{err}");
    assert!(err.contains("unknown primitive <div>"), "{err}");
    assert!(err.contains("JSX fragments are not supported"), "{err}");
    assert!(
        err.contains("component calls must be written as JSX tags"),
        "{err}"
    );

    let src = r#"
        function App() {
            return (
                <View>
                    <div />
                    {call()}
                    <Show><Text>not arrow</Text></Show>
                </View>
            );
        }
        export default App;
    "#;
    let err = errors("multi.tsx", src);
    assert!(err.contains("unknown primitive <div>"), "{err}");
    assert!(
        err.contains("component calls must be written as JSX tags"),
        "{err}"
    );
    assert!(
        err.contains("control flow child must be an arrow function"),
        "{err}"
    );
}

#[test]
fn limits_errors_to_100() {
    let tags: String = (0..120).map(|i| format!("        <bad{i} />\n")).collect();
    let src = format!(
        r#"
        function App() {{ return <View>
{tags}        </View>; }}
        export default App;
    "#
    );
    let err = errors("many.tsx", &src);
    assert!(err.contains("too many errors, stopping at 100"), "{err}");
    assert_eq!(err.matches("many.tsx:").count(), 100, "{err}");
}

#[test]
fn validates_view_prop_types() {
    let src = r#"function App() { return <View backgroundColor={true} />; } export default App;"#;
    let err = errors("type.tsx", src);
    assert!(
        err.contains("backgroundColor") && err.contains("type mismatch"),
        "{err}"
    );

    let src = r#"function App() { return <View flexDirection="nope" />; } export default App;"#;
    let err = errors("enum.tsx", src);
    assert!(
        err.contains("flexDirection") && err.contains("type mismatch"),
        "{err}"
    );
}

#[test]
fn allows_correct_prop_types() {
    let src = r#"
        function App() {
            return (
                <View backgroundColor={0xff} flexGrow={1} padding={[12, 8, 8, 12]}>
                    <View backgroundColor="oklch(0.2 0.01 264)" flexDirection="row" />
                    <Text color="white" fontSize={18} />
                </View>
            );
        }
        export default App;
    "#;
    assert!(compile_module("ok.tsx", src, &CompileOptions::default()).is_ok());
}

#[test]
fn checks_required_props() {
    let src = r#"function App() { return <Image />; } export default App;"#;
    let err = errors("req.tsx", src);
    assert!(err.contains("missing required prop 'src'"), "{err}");
}

#[test]
fn rejects_unknown_props() {
    let src = r#"function App() { return <View unknownProp={1} />; } export default App;"#;
    let err = errors("unk.tsx", src);
    assert!(err.contains("unknown prop 'unknownProp'"), "{err}");
}

#[test]
fn optimizes_literal_props() {
    let src = r#"
        function Label(props) {
            return <Text fontSize={props.fontSize()}>{props.label()}</Text>;
        }
        function App() {
            return <Label fontSize={18} label="x" />;
        }
        export default App;
    "#;
    let code_default = compile("row.tsx", src);
    assert!(
        code_default.contains("() => 18"),
        "default wraps:\n{code_default}"
    );

    let code_opt = compile_module(
        "row.tsx",
        src,
        &CompileOptions {
            optimize: true,
            ..CompileOptions::default()
        },
    )
    .unwrap()
    .code;
    assert!(!code_opt.contains("() => 18"), "opt skips:\n{code_opt}");
    assert!(
        code_opt.contains("fontSize: 18"),
        "opt inlines:\n{code_opt}"
    );
}

#[test]
fn skips_dynamic_for_static_expr() {
    let src = r#"
        import { signal } from "bubble-shell";
        function App() {
            const count = signal(0);
            return <View width={10 + 5} height={count()} />;
        }
        export default App;
    "#;
    let code_default = compile("static.tsx", src);
    assert!(
        code_default.contains("width: 10 + 5"),
        "static arithmetic is not a signal read:\n{code_default}"
    );
    assert!(
        !code_default.contains("__bubble_rt_dynamic(() => 10 + 5)"),
        "default does not wrap a constant:\n{code_default}"
    );
    assert!(
        code_default.contains("__bubble_rt_dynamic(() => count())"),
        "signal reads stay dynamic:\n{code_default}"
    );

    let code_opt = compile_module(
        "static.tsx",
        src,
        &CompileOptions {
            optimize: true,
            ..CompileOptions::default()
        },
    )
    .unwrap()
    .code;
    assert!(
        !code_opt.contains("__bubble_rt_dynamic(() => 10 + 5)"),
        "opt skips:\n{code_opt}"
    );
    assert!(
        code_opt.contains("__bubble_rt_dynamic(() => count())"),
        "opt still binds signal reads:\n{code_opt}"
    );
}

#[test]
fn rejects_rest_params_in_worklet() {
    let src = r#"const fn = (...args) => { "worklet"; }; export default fn;"#;
    let err = errors("wl-rest.tsx", src);
    assert!(
        err.contains("worklet parameters must be plain identifiers"),
        "{err}"
    );
}

#[test]
fn enforces_worklet_local_limit() {
    let decls: String = (0..17).map(|i| format!("const x{i} = {i};\n")).collect();
    let src = format!(r#"const fn = () => {{ "worklet"; {decls} }}; export default fn;"#);
    let err = errors("wl-locals.tsx", &src);
    assert!(err.contains("more than 16 locals"), "{err}");
}

#[test]
fn rejects_non_value_assignment_in_worklet() {
    let src = r#"const fn = (sv) => { "worklet"; sv.other = 1; }; export default fn;"#;
    let err = errors("wl-assign.tsx", src);
    assert!(err.contains("only write shared values (.value)"), "{err}");
}

#[test]
fn rejects_writing_to_worklet_param() {
    let src = r#"const fn = (sv) => { "worklet"; sv.value = 1; }; export default fn;"#;
    let err = errors("wl-param.tsx", src);
    assert!(err.contains("parameters are read-only"), "{err}");
}

#[test]
fn detects_worklet_recursion() {
    let src = r#"const a = () => { "worklet"; b(); }; const b = () => { "worklet"; a(); }; export default a;"#;
    let err = errors("wl-rec.tsx", src);
    assert!(err.contains("worklet recursion is not supported"), "{err}");
}

#[test]
fn rejects_non_literal_with_timing_target() {
    let src = r#"const slide = 0; const fn = () => { "worklet"; slide.value = withTiming(); }; export default fn;"#;
    let err = errors("wl-timing.tsx", src);
    assert!(err.contains("withTiming needs a target value"), "{err}");
}

#[test]
fn rejects_invalid_easing() {
    let src = r#"const slide = 0; const fn = () => { "worklet"; slide.value = withTiming(10, { easing: "invalid" }); }; export default fn;"#;
    let err = errors("wl-easing.tsx", src);
    assert!(
        err.contains("tween easing must be \"linear\" or \"easeInOut\""),
        "{err}"
    );
}

#[test]
fn rejects_non_literal_with_spring() {
    let src = r#"const slide = 0; const fn = () => { "worklet"; slide.value = withSpring(); }; export default fn;"#;
    let err = errors("wl-spring.tsx", src);
    assert!(err.contains("withSpring needs a target value"), "{err}");
}

#[test]
fn rejects_non_shared_value_in_cancel_animation() {
    let src = r#"const fn = () => { "worklet"; cancelAnimation(); }; export default fn;"#;
    let err = errors("wl-cancel.tsx", src);
    assert!(
        err.contains("cancelAnimation needs a shared value"),
        "{err}"
    );
}

#[test]
fn rejects_non_function_in_schedule_on_js() {
    let src = r#"const fn = () => { "worklet"; scheduleOnJS(123); }; export default fn;"#;
    let err = errors("wl-sched.tsx", src);
    assert!(
        err.contains("scheduleOnJS must pass a function reference"),
        "{err}"
    );
}

#[test]
fn rejects_unsupported_statement_in_worklet() {
    let src = r#"const fn = () => { "worklet"; try {} catch (e) {} }; export default fn;"#;
    let err = errors("wl-try.tsx", src);
    assert!(
        err.contains("worklet statements must be expressions, let/const, if/else, or return"),
        "{err}"
    );
}

#[test]
fn rejects_non_numeric_worklet_literal() {
    let src = r#"const fn = () => { "worklet"; return 1e999; }; export default fn;"#;
    let err = errors("wl-inf.tsx", src);
    assert!(err.contains("worklet numbers must be finite"), "{err}");
}

#[test]
fn rejects_spread_in_worklet_call() {
    let src = r#"const fn = () => { "worklet"; return Math.min(...[1, 2]); }; export default fn;"#;
    let err = errors("wl-spread.tsx", src);
    assert!(
        err.contains("spread arguments are not worklet expressions"),
        "{err}"
    );
}

#[test]
fn rejects_non_object_literal_worklet_options() {
    let src = r#"const opts = {}; const fn = () => { "worklet"; slide.value = withTiming(10, opts); }; export default fn;"#;
    let err = errors("wl-opts.tsx", src);
    assert!(
        err.contains("worklet animation options must be an object literal"),
        "{err}"
    );
}

#[test]
fn rejects_non_numeric_worklet_option_values() {
    let src = r#"const slide = 0; const fn = () => { "worklet"; slide.value = withTiming(10, { duration: "100" }); }; export default fn;"#;
    let err = errors("wl-dur.tsx", src);
    assert!(
        err.contains("worklet animation options must be numeric"),
        "{err}"
    );
}

#[test]
fn slot_requires_content() {
    let src = r#"function App() { return <Slot />; } export default App;"#;
    let err = errors("slot-empty.tsx", src);
    assert!(
        err.contains("requires a function child or an of prop"),
        "{err}"
    );
}

#[test]
fn slot_rejects_unknown_prop() {
    let src = r#"function App(props) { return <Slot of={props.x} unknown="val" />; } export default App;"#;
    let err = errors("slot-prop.tsx", src);
    assert!(err.contains("<Slot> has no property unknown"), "{err}");
}

#[test]
fn slot_rejects_jsx_child() {
    let src = r#"function App() { return <Slot><View /></Slot>; } export default App;"#;
    let err = errors("slot-jsx.tsx", src);
    assert!(err.contains("expects a render function value"), "{err}");
}

#[test]
fn slot_rejects_multiple_children() {
    let src = r#"function App() { return <Slot>{() => {}}{() => {}}</Slot>; } export default App;"#;
    let err = errors("slot-multi.tsx", src);
    assert!(err.contains("single function child"), "{err}");
}

#[test]
fn slot_rejects_text_child() {
    let src = r#"function App() { return <Slot>text</Slot>; } export default App;"#;
    let err = errors("slot-text.tsx", src);
    assert!(
        err.contains("children must be a render function value"),
        "{err}"
    );
}

#[test]
fn match_outside_switch_errors() {
    let src = r#"function App() { return <Match when={true}><Text>Hi</Text></Match>; } export default App;"#;
    let err = errors("match-out.tsx", src);
    assert!(err.contains("Match must be a child of Switch"), "{err}");
}

#[test]
fn switch_rejects_non_match_children() {
    let src = r#"function App() { return <Switch><View /></Switch>; } export default App;"#;
    let err = errors("switch-child.tsx", src);
    assert!(err.contains("Switch children must be Match"), "{err}");
}

#[test]
fn match_requires_when_prop() {
    let src = r#"function App() { return <Switch><Match><Text>Hi</Text></Match></Switch>; } export default App;"#;
    let err = errors("match-when.tsx", src);
    assert!(err.contains("requires a when prop"), "{err}");
}

#[test]
fn style_expr_rejects_unknown_function() {
    let src = r#"function App() { return <ViewStyle target={1} opacity={unknown()} />; } export default App;"#;
    let err = errors("style-fn.tsx", src);
    assert!(err.contains("is not a style expression function"), "{err}");
}

#[test]
fn style_expr_rejects_non_builtin_callee() {
    let src = r#"function App() { return <ViewStyle target={1} opacity={Math.min(1, 2)} />; } export default App;"#;
    let err = errors("style-call.tsx", src);
    assert!(
        err.contains("style expressions only support builtin functions"),
        "{err}"
    );
}

#[test]
fn style_expr_rejects_spread() {
    let src = r#"function App() { return <ViewStyle target={1} opacity={min(...[1])} />; } export default App;"#;
    let err = errors("style-spread.tsx", src);
    assert!(
        err.contains("spread arguments are not style expressions"),
        "{err}"
    );
}

#[test]
fn style_expr_rejects_signal_read() {
    let src = r#"import { signal } from "bubble-shell"; function App() { const s = signal(1); return <ViewStyle target={1} opacity={s()} />; } export default App;"#;
    let err = errors("style-sig.tsx", src);
    assert!(
        err.contains("read signals; declare a shared value"),
        "{err}"
    );
}

#[test]
fn style_requires_target() {
    let src = r#"function App() { return <ViewStyle />; } export default App;"#;
    let err = errors("style-tgt.tsx", src);
    assert!(err.contains("requires a target prop"), "{err}");
}

#[test]
fn textfield_rejects_non_scrollbar_children() {
    let src = r#"function App() { return <TextField><View /></TextField>; } export default App;"#;
    let err = errors("tf-child.tsx", src);
    assert!(
        err.contains("children must be style declarations or ScrollBar"),
        "{err}"
    );
}

#[test]
fn image_rejects_children() {
    let src =
        r#"function App() { return <Image src="a.png"><View /></Image>; } export default App;"#;
    let err = errors("img-child.tsx", src);
    assert!(err.contains("children must be style declarations"), "{err}");
}

#[test]
fn view_rejects_bare_text() {
    let src = r#"function App() { return <View>text</View>; } export default App;"#;
    let err = errors("view-text.tsx", src);
    assert!(err.contains("text nodes belong inside Text"), "{err}");
}

#[test]
fn empty_jsx_expression_errors() {
    let src = r#"function App() { return <View>{}</View>; } export default App;"#;
    let err = errors("empty.tsx", src);
    assert!(err.contains("empty JSX expression"), "{err}");
}

#[test]
fn namespaced_attribute_errors() {
    let src = r#"function App() { return <View ns:attr="val" />; } export default App;"#;
    let err = errors("ns.tsx", src);
    assert!(
        err.contains("namespaced JSX attributes are not supported"),
        "{err}"
    );
}

#[test]
fn duplicate_children_prop_errors() {
    let src = r#"function Row(props) { return <Card children={() => {}} children={() => {}} />; } export default Row;"#;
    let err = errors("dup-child.tsx", src);
    assert!(err.contains("duplicate children prop"), "{err}");
}

#[test]
fn both_children_prop_and_jsx_children_errors() {
    let src = r#"function Row(props) { return <Card children={() => {}}><View /></Card>; } export default Row;"#;
    let err = errors("both-child.tsx", src);
    assert!(
        err.contains("cannot have both a children prop and JSX children"),
        "{err}"
    );
}
