# 09 · JSX 转换方案 v2：Solid 一致的惰性元素与统一 getter props

状态：方案已定稿，未实现。本文不引入任何新的验证工具或设备矩阵；所有验证仍限于 `cargo test --workspace`、`cargo check --workspace`、`node --test examples/web/*.test.mjs` 与手动 demo 冒烟（`node examples/web/m0-smoke.mjs <fixture>`）。

实现落点：`crates/compiler/src/lib.rs`（绑定扫描扩展、props/children 降级规则重写）、`crates/native/js/runtime.js`（新增 `content()` 分支原语、`Dynamic` 收回内部）、`examples/app/fixtures/*.tsx` 与 `crates/testing/apps/*.tsx`（按新规则迁移）、`crates/compiler/tests/lower.rs`（用例更新）、`docs/07-js-bundling-plan.md` 与本文件（术语同步）。

## 1. 目标与边界

消除现行 JSX 降级的三类系统性缺陷（均已在 gestures 演示中实际踩中）：

- **表达式容器静默文本化**：View/gesture children 中 `{expr}` 一律编译为 `new Text({}, () => String(expr))`。元素值表达式（`{pad}`）变成字符串垃圾；裸标识符退化为静态 `String(pad)`（打印函数源码）。
- **求值即挂载的 parent 陷阱**：`const pad = <Pad/>` 在声明处立即挂载到"当前 parent"，存入变量后再用会挂错位置或双挂载。
- **Dynamic 协议半途而废**：`dynamic()` 只在原语边界被 `bindProp` 完整消费（create→bind→dispose）；组件边界无人消费，组件作者被迫 `instanceof Dynamic` 手工解包。且 `reads_signal` 只追踪顶层 `const x = signal()/computed()` 绑定，函数返回的 signal、props 转发的 signal 全部漏包——同一语法形状因表达式是否恰好命中启发式而呈现不同运行时形态。

架构前提（不在本轮讨论）：组件只调用一次、无 VDOM 重渲染；引擎节点是 wasm 句柄，挂载是副作用；Show/For/Switch 拥有节点替换权。所有改动都建立在这个细粒度模型之上，与 Solid 的编译模型对齐。

明确不在本轮：signal 可调用 API 本身（保持 `signal()` 返回带 `.update/.set` 的读取函数）、样式/变量系统（`createVar`/`styleExpr`/`select` 等 style 内建）、runtime 两阶段节点描述符（决策 4 否决的 C 案）、dev/hot 路径重构。

## 2. 已确认的设计决策

| # | 决定 |
|---|------|
| 1 | 组件 props 采用 **Solid 式统一 getter**：所有非 handler 的 prop 表达式一律编译为 `() => expr`，组件统一 `props.x()` 读取。`Dynamic` 收回 runtime 内部（仅原语 `bindProp`/`bindGestureProp` 消费），从公共导出中移除 |
| 2 | **裸 signal 绑定传参编译报错**：`live={live}` 报错并提示 `live={live()}`；编译器已有 signal 绑定追踪，可静态检出"prop 表达式整体是一个 signal 绑定标识符" |
| 3 | JSX 元素表达式的值语义改为**编译器统一惰性化**：元素表达式的值是惰性渲染 thunk，编译器在使用点插入调用。runtime 原语保持即时挂载，不引入节点描述符 |
| 4 | View/gesture children 中 `{expr}` 三分类：元素类 → 新 runtime `content(thunk)` 分支；字符串/值类 → 就地响应式 Text（现有行为，语义变为正确）；slot 引用 → `slot()` 不变 |
| 5 | 函数调用返回元素（`{make()}` 在 children 位置且无法静态分类）→ 编译报错："组件调用必须是 JSX 标签或元素绑定" |

## 3. 新降级规则

### 3.1 绑定扫描扩展（编译期分类的基础）

现有 `visit_program`/`visit_function`/`visit_arrow_function_expression` 收集 `const x = signal(...) | computed(...)` 进 scopes。扩展为同时收集**元素绑定**：`const x = <JSX/>` 或初始化表达式含 JSX 的 const。扫描结果是一个绑定分类表：

- `signal` 绑定：用于 `reads_signal`（原语 props 包装）与裸传参报错。
- `element` 绑定：用于 children 中 `{x}` 的 content 分支分类。

限制（记录在案，不做数据流分析）：仅覆盖顶层与函数/箭头体内的 `const` 声明；经函数参数、函数返回值、对象属性流转的元素值不进入分类，命中"函数调用返回元素"报错或被当作字符串值处理。这条边界与 signal 追踪的既有边界一致。

### 3.2 组件 props

- 非 handler 表达式 → `() => expr`（常量也一样，getter 统一；handler 即箭头/函数表达式原样传递）。
- JSX 值 → thunk（现有 `slot_thunk_of` 行为不变，惰性化后自然成立）。
- 整体是一个 signal 绑定标识符 → 编译报错，提示 `x={sig()}`。
- `children` prop：维持现有"JSX 或函数值或 `props.children` 转发"三选一规则。
- runtime `component()` 不变；`Dynamic` 类停止导出（runtime 内部对原语 props 的包装保留：读 signal 的原语 prop 仍编译为 `dynamic(() => expr)`，由 `bindProp`/`bindGestureProp` 消费）。

### 3.3 children 分类（`children_block`，View 与手势原语共用）

按序处理每个 child，输出语句序列：

| child | 降级 |
|---|---|
| JSX 元素 | `lower_element` 后**立即调用**（保持挂载顺序与 parent 上下文） |
| `{props.children}` / slot 引用 | `slot(thunk)`（不变） |
| `{expr}` 且 expr 含 JSX、或解析为元素绑定、或为显式 thunk | `content(thunk)`：绑定运行，依赖变化时 dispose 旧分支、重放 thunk |
| `{expr}` 其他（字符串/数值/读 signal 的值表达式） | `new Text({}, () => expr)`——现行为，就地更新 |
| `{make()}` 等无法静态分类为元素的函数调用 | 编译报错（决策 5） |
| 裸文本 | 维持报错 "text nodes belong inside Text" |

字符串走 `Text` 而非 `content()`，保证 `{count()}` 这类高频文本仍是廉价的原地更新；`content()` 只承担节点级替换（复用 runtime `replaceBranch`，即 Show 的内部机制，含虚拟 scope 与 dispose）。

### 3.4 惰性化的编译器实现

`visit_expression` 对 JSX 的降级从"就地展开为 component/primitive 调用"改为"展开为 thunk"。使用点负责调用：

- `children_block` 的元素 child：展开后立即调用（`(( ) => (…))( )` 或直接保留现有立即降级路径——实现取"声明处不展开、使用处展开"的最简方案：绑定扫描登记后，降级延迟到使用点）。
- 组件 props 的 JSX：已是 thunk（不变）。
- `<Show>`/`<For>`/`<Switch>` 的渲染回调：内部 JSX 在回调体内使用点调用。
- `const pad = <Pad/>`：`pad` 绑定为 thunk；`{pad}` → `content(pad)`；`pad` 被当作普通值使用（如插值）→ 编译报错提示用 `<Slot of={pad}/>` 或组件。

## 4. runtime 改动

- 新增 `content(thunk)` 导出：创建 anchor + 虚拟 scope + 绑定（同 `show` 的 `replaceBranch` 路径），渲染 thunk 构建的节点；依赖变化时 dispose 重建。字符串快速路径不需要（编译期已分流）。
- `Dynamic`、`dynamic()` 停止导出，移入内部（`bindProp` 继续使用）。`gesturePropValue`/`bindGestureProp` 不变。
- `slot()`、控制流四组件、`mount()`、事件/gesture/edit/var 派发均不变。

## 5. 迁移清单

新规则是编译期破坏性变更，迁移与实现同一批完成：

1. `examples/app/fixtures/`：6 个 fixture。gestures.tsx 可回退为自然写法（删掉 `SceneFrame`/`unwrap`/`Dynamic` 解包样板：`live={live()}` 直接传 getter，`{pad}` 类组合直接写）。
2. `crates/testing/apps/`：6 个 TSX 迁移；`scripts/*.script` 断言不涉及 props 形态，预期零改动。
3. `crates/compiler/tests/lower.rs`：断言更新为 getter/content 形态。
4. 文档：`docs/07-js-bundling-plan.md` 的降级规则章节指向本文件；`docs/reference/embedding.md` 术语核对。

## 6. 验证

- `cargo test --workspace`（lower.rs 覆盖新规则：getter props、裸 signal 报错、`{pad}` content 分支、字符串就地 Text、函数调用返回元素报错）。
- `node --test examples/web/*.test.mjs`。
- `node examples/web/m0-smoke.mjs counter|gestures`（gestures 演示迁移后行为不变：tap 日志、Pan 场景拖动读数）。
- 手动 demo 冒烟：浏览器打开 `?fixture=gestures` 点按/拖动。

## 7. 风险与开放问题

- 绑定扫描的浅数据流边界（§3.1）：跨函数流转的元素值会被误分类为字符串值，报错信息需给出明确出路（组件/thunk）。
- `content()` 重建成本：节点级替换每次全量重建；高频更新必须走 `<Text>` 字符串路径。文档需写明这条性能分界。
- getter 统一后，组件读取静态 prop 也多一次调用；如 profile 显示开销显著，可在编译期对纯字面量 prop 直传（协议不变，仅优化）。
