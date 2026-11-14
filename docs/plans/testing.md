# 测试方案：取代 crates/eval

参照 `../nsh` 的 `nsh-testing`（驱动库）+ `tests/compat`（声明式脚本）两层架构。
本方案已在讨论中逐项拍板；"决策记录"一节即最终结论。

## 决策记录

| # | 决定 |
|---|------|
| 1 | 分层：`bubble-testing` Rust 驱动库为核心；行式声明脚本作为其上的数据格式，本期一起落地 |
| 2 | `crates/eval` 整体删除（含 `device.rs`、m1 场景、报告类型、`bubble-eval` 二进制） |
| 3 | 驱动层接 C ABI（`bubble-ffi`）：被测面就是 iOS/Android 宿主实际嵌入的函数集。选择器基于 `bubble_semantics` JSON 实现，零新增 ABI。C ABI 是 thread-local 全局绑定 → 场景串行，可接受 |
| 4 | 首批覆盖全部 fixture：counter、editing、gestures、focus、media、slot 各一个 headless 场景 |
| 5 | `testID` 全链路：compiler → props → core → semantics → 选择器 `id:` |
| 6 | 独立测试 fixture 放 `crates/testing/apps/`（TSX）；`examples/` 已整体删除（2026-11-14），演示 app 后续以独立项目重建 |
| 7 | 脚本格式：行式文本（`tap id:next` / `expect Count: 1` / `#` 注释），仿 nsh `script.txt` |
| 8 | 删除 eval 后的孤儿模拟器测试代码一并删：`examples/ios/UITests*`、`examples/android/app/src/androidTest` |

## 架构

```mermaid
flowchart LR
  subgraph crates/testing (bubble-testing)
    apps/*.tsx -- bundle_entry(进程内) --> B[bundle code + 生成的 manifest]
    scripts/*.script --> R[script runner]
    D[driver: Session + 选择器 + 帧泵]
    R --> D
    T[cargo test 入口: 每个脚本一个测试用例]
  end
  D -- bubble_* C ABI --> N[bubble-ffi / native]
  B -- bubble_load --> D
```

### crate 布局（`crates/testing`，包名 `bubble-testing`）

```
crates/testing/
  src/
    driver.rs      # Session：C ABI 封装、选择器、输入步骤、帧泵
    semantics.rs   # SemanticsSnapshot 反序列化 + 查询（按 id/text/role）
    bundle.rs      # 进程内构建测试 fixture（LazyLock，每 app 一次）
    manifest.rs    # 由 BundledEntry 生成 manifest JSON（sha256/size）
    script.rs      # 脚本解析器 + 解释执行
  apps/            # 独立测试 TSX：counter/editing/gestures/focus/media/slot
  scripts/         # 每个脚本对应 apps 里的一个入口
  tests/scripts.rs # 集成测试：枚举 scripts/ 目录，逐个跑（失败即断言失败）
```

- 依赖：`bubble-ffi`、`bubble-bundler`、`sha2`。全部本地，离线。
- 串行：C ABI 是 thread-local 绑定，`cargo test` 天然逐用例执行；帧泵线程用现有 `bubble_bind(id)` 模式（参照 eval `frame_detached`）。
- fixture 构建：`bubble_bundler::bundle_entry` 进程内调用，`LazyLock` 每 app 构建一次（rolldown/oxc 均为本地 Rust 依赖）。构建模式用开发档，产物喂 `bubble_load`。
- manifest：程序化生成（schema/id/version/minRuntime/platforms/entry/files[{path,sha256,size}]/assets/占位签名），sha256 由 `sha2` 计算，与 `counter.json` 字段一致。

### 驱动 API（Rust 层）

选择器来自 `bubble_semantics` JSON（`SemanticsNode`: handle/role/label/value/bounds/actions/…）。
Text 节点 label 取自内容、Field 取 value——text 选择器现成可用。

```rust
let mut app = Session::start(&COUNTER)?;      // load bundle + manifest
app.wait_for("Count: 0")?;                    // 默认 2s 超时，每 10ms 泵帧
app.tap(id("increment"))?;                    // semantics bounds 中心，tap down/up
app.type_text("hello")?;                      // 编辑事务写聚焦 field
app.scroll(id("list"), 0.0, 40.0)?;
assert_eq!(app.text_of(id("label"))?, "Count: 1");
```

要点：
- `Session` 内部保存 shell id；步骤调用后泵帧（`bubble_frame`）驱动布局/手势推进。
- 每次 create 前 `bubble_destroy()`（现状语义），进程内同一时刻一个活动 shell。
- 断言失败即测试失败（fail-fast），消息带 semantics 快照摘录，替代旧 `SuiteReport`。

### testID 链路（唯一动核心/编译器的改动）

- `crates/core/src/props.rs`：`Prop::TestID`，`parse("testID")`（仿 `Label`）。
- core 节点存储 `test_id`；`SemanticsNode` 加 `test_id` 字段，进 `semantics_json`。
- 编译器：props 按名透传（`label`/`role` 即此模式），预计零改动，实现时验证。
- 测试 fixture 的 TSX 写 `testID="increment"`。

### 脚本 DSL

```
# 计数器
expect Count: 0
tap id:increment
expect Count: 1
scroll id:list 0 40
type "hello"
absent This text is gone
eval globalThis.ticks = 0
post-eval while (true) {}
expect-faults >= 1
expect-status running
host-reply {"ok":true}
visible off
```

- 命令集以 fixture 实际内容收敛：`tap`/`type`/`scroll`/`expect`/`absent`/`eval`/`post-eval`/`host-reply`/`visible`/`expect-faults`/`expect-status`。
- `expect`/`absent` 内建等待（默认 2s），泵帧由 runner 承担——dead_loop 场景由帧泵线程覆盖，不再需要 eval 里的手工 detached 线程编排。
- 复杂断言（脚本表达不了的）直接写成同 crate 的 Rust 测试——分层设计的出口，不允许为 DSL 扩展特例命令。

## 删除清单

- `crates/eval/` 整体 + workspace 成员登记。
- `examples/ios/UITests`、`examples/ios/BubbleShellUITests.xcodeproj`。
- `examples/android/app/src/androidTest/`。
- `docs/roadmap.md` 中 `cargo run -q -p bubble-eval -- …` 设备命令引用（MVP 验收命令表随之更新）。
- `AGENTS.md` 中 bubble-eval frozen-harness 表述、其他 docs/README 提及（实现时 grep 全仓）。

## 验证（MVP 预算内）

- `cargo test --workspace`：新套件 + 其余 crate 测试，目标 <10s、离线、无设备。风险点：进程内 rolldown 构建耗时——用 `LazyLock` 复用、开发档产物、实现时实测；超预算则改为 build.rs 构建产物缓存。
- `node --test sdk/web/*.test.mjs`：单测随 `examples/web` 一起迁到 `sdk/web/`。
- 手动 smoke 流程不变（模拟器自动化入口随 eval 删除，属已拍板范围）。

## 阶段划分（每步跑 `cargo test --workspace`）

1. testID 链路（props → core → semantics）+ fixture TSX 冒烟。
2. `bubble-testing` 驱动层 + counter 垂直切片（脚本 + 测试入口）。
3. 其余五个 fixture 的 apps + scripts。
4. 删除 `crates/eval`、孤儿 instrumentation、文档引用更新。

## 显式延后（post-MVP backlog）

- 并行多 session（驱动层接 native 内部 Rust API）：出现场景数超时再做。
- 脚本 DSL 命令集扩充、nsh compat 式 API 冻结记录（`nsh-compat` 仓库模式）。
- 已拍板（2026-11-14）：`examples/` 整体删除；演示与测试 app 不再双份，fixture 只保留 `crates/testing/apps/`，演示 app 重建时为独立项目。
- 设备/模拟器自动化、性能门槛等原延后项不变。
