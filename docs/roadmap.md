# 路线图

## 交付策略：先做出 MVP

MVP 是一个可运行、可交互的演示，不是验收程序。MVP 完成前不执行需要人工介入、实体设备、真实读屏软件或长时间压测的验证；这些全部进入延后验证 backlog（见 [risks.md](risks.md)）。

### MVP 范围

- bundle/manifest 加载与验签占位。
- 编译器 MVP：TSX/TS/JS 函数组件 → 原生渲染调用；Oxc 去类型并降 JSX，rolldown 链接成单 ESM。信号图在 Rust；不做 VDOM、模板 IR 或预编译子树。
- 打包：`bubble bundle` / `bubble dev`；`bubble-shell` / `bubble:rt` 由宿主提供；资源按 `assets/<name>.<hash8>.<ext>` 落盘；Web 开发态 HMR。详见 [adr/js-bundling.md](adr/js-bundling.md)。
- Rust 保留树 + Taffy 布局；op 协议 + 双缓冲提交。
- Web：主线程一份 WASM + JS 回放 `PaintCmd`；无 Worker。
- Native：QuickJS、C ABI、事件循环、超时/内存限制；iOS CoreGraphics、Android Canvas painter；宿主能力桥与生命周期。
- 输入：现有手势、编辑事务、焦点与语义桥实现保持可用。
- 示例 fixture：`counter`、`editing`、`gestures`、`focus`。

### MVP 验收

- `cargo test --workspace` 通过，离线、无需设备，目标 10 秒内。
- `cargo check --workspace` 通过。
- `node --test examples/web/*.test.mjs` 通过。
- Web 页面手动 smoke：渲染、点击、滚动、文本框输入、焦点移动各一次。
- `bubble dev` 下修改组件文本，页面更新且计数状态保留（Web）。
- iOS/Android 模拟器启动同一 fixture，完成点击、滚动、一次文本输入。
- 现有模拟器 harness 可以作为可选回归入口运行，但不是 MVP 发布门槛：
  `cargo run -q -p bubble-eval -- gestures-ios|gestures-android|editing-ios|editing-android`。

### 明确不属于 MVP 验收

- 实体设备、厂商键盘、真实 VoiceOver / TalkBack / NVDA。
- 首帧、60fps、内存、包体等性能门槛。
- 更新回滚、坏签名、断网等故障演练。
- golden/screenshot、fuzz、conformance 套件与跨端验收矩阵。
- 验收报告、设备信息快照、截图附件。

## 里程碑

### M0 · Web 核心闭环（已实现）

范围：

- bundle/manifest 加载与验签占位。
- 编译器 MVP：TSX 函数组件 → 原生渲染调用。M0 的历史实现是单文件直译；现已按 [adr/js-bundling.md](adr/js-bundling.md) 迁移到 Oxc + rolldown 单 ESM。信号图在 Rust。不做 VDOM、模板 IR 或预编译子树。
- Rust 保留树 + Taffy 布局。
- op 协议 + 双缓冲提交。
- Web：主线程一份 WASM + JS 回放 `PaintCmd`。无 Worker。

验收以 MVP 验收命令和 Web 手动 smoke 为准；不再记录帧率、首帧或设备报告。

### M1 · Native 核心（已实现）

范围：

- QuickJS 集成、C ABI、事件循环、超时/内存限制。
- iOS CoreGraphics、Android Canvas painter。
- 宿主能力桥、单实例嵌入、生命周期。
- 宿主注入字体。

验收以同一 bundle 在 iOS/Android 模拟器渲染与交互为准；实体设备与性能基线延后。

### M2 · 输入与无障碍（实现完成，验证分层）

范围：

- 手势与滚动协商（[adr/gestures.md](adr/gestures.md)）。
- 自绘输入模型、Android IME、iOS `UITextInput`、Web 隐藏 textarea（[adr/input-model.md](adr/input-model.md)）。
- 焦点树、Tab/Shift-Tab、Enter/Space、Esc、modal 焦点陷阱。
- Rust 语义树 + 平台 a11y 桥 + Web ARIA 覆盖层。
- 选区手柄、编辑菜单、读屏滚动 action。

MVP 内：

- 核心行为由 `cargo test --workspace` 覆盖。
- Web 交互与键盘路由由 Node 单测覆盖。
- iOS/Android 模拟器保留基础启动与手势/编辑 harness，作为可选回归。
- 不新增可访问性验收套件，不要求真实读屏软件或实体设备。

延后：

- VoiceOver / TalkBack / NVDA 核心路径实测。
- 实体设备、厂商键盘、iOS 外接键盘差异。
- 任意双指轨迹、第三指、按住期间系统中断等公共 XCTest 表达受限的场景。

### M3 · 更新、观测与性能（post-MVP）

范围：

- Ed25519 manifest、内容寻址、LKG、宿主激活、自动回退。
- 灰度、kill switch、兼容性拒绝。
- 结构化埋点、帧统计、错误上报。
- 性能加固与包体优化。
- 打包优化：QuickJS 字节码、minify、代码分割、发布 source map、native 状态保留 HMR 与 WebSocket dev host。

M3 在 MVP 之后启动；其故障演练、性能门槛和更新回滚验证同样按延后验证 backlog 执行。

## 当前状态

- 版本：v1 架构草案
- 日期：2026-11-13
- 阶段：MVP 优先；JS 打包已按 [adr/js-bundling.md](adr/js-bundling.md) 切到 Oxc + rolldown 单 ESM，`bubble bundle` / `bubble dev` 是构建入口。实体设备、真实读屏、性能与故障演练进入 post-MVP backlog，不作为当前验收。
- 活跃计划：[plans/shared-variables.md](plans/shared-variables.md) 正在实现中。
