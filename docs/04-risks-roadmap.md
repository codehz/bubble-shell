# 04 · 风险与路线

## 1. 风险

| 风险 | 级别 | 影响 | 缓解 |
|---|---|---|---|
| QuickJS 事件循环、async、调试器需自建 | 高 | 开发效率与线上排障 | 基于 quickjs-ng/libc；先做最小事件循环；调试协议与 conformance 延后 |
| Oxc `0.61 → 0.158` 与 rolldown 迁移 | 中 | 编译器重写、构建变慢 | 按 [07](07-js-bundling-plan.md) 分步；rolldown 只作构建依赖；测试用最小 fixture |
| 主线程脚本卡住绘制 | 中 | Web 长任务掉帧，且无法强杀 | 接受。软预算只埋点；不做 Worker、不做切片 |
| a11y/IME 是最大平台差异面 | 高 | 产品可用性与合规 | MVP 只交付语义树与平台桥；VoiceOver/TalkBack/NVDA 验收延后 |
| Android IME 与自绘编辑状态语义不一致 | 高 | CJK 组合、选区、删除与光标位置失败 | 按 [自绘输入模型重设计计划](06-input-model-plan.md) 锁步迁移编辑事务；模拟器验证；真机与厂商键盘延后 |
| 全表面重绘性能 | 中 | 60fps 门槛 | 脏矩形、静态子树缓存、帧耗时埋点；性能门槛延后 |
| 远程 JS 的 App Store 合规 | 高 | iOS 上线 | 第一方签名、无 JIT；准备 data-only 降级；提前审核沟通 |
| FFI / QuickJS 内存安全 | 高 | 宿主崩溃 | 安全封装、panic 屏障、配额、灰度；fuzz 延后 |
| 三端文本布局差异（已接受） | 中 | 视觉/高度不一致 | 接受差异；布局不依赖固定文本高度；golden 快照延后 |
| 密钥管理与回滚 | 中 | 供应链安全 | `keyId` 轮换、吊销、单调版本、LKG；回退演练延后 |
| Web CORS / 鉴权 | 中 | 数据获取受限 | 公开内容 runtime fetch；带凭证 API 走宿主代理 |
| QuickJS 性能低于 JIT 引擎 | 中 | 复杂脚本卡顿 | 事件粒度小、重计算下沉宿主；性能基线延后 |

## 2. v1 明确不做

- 多实例、差量更新、grid/完整 CSS、高级视觉效果。
- 宿主原生组件嵌入、能力授权 UI、统一调试器、桌面端。
- minify、代码分割、QuickJS 字节码、发布 source map。
- native 状态保留 HMR 与 WebSocket dev host；native 开发仍重新打包并重启。
- 跨端像素级一致；字体内置；运行时自带字体。

## 3. 交付策略：先做出 MVP

MVP 是一个可运行、可交互的演示，不是验收程序。MVP 完成前不执行需要人工介入、
实体设备、真实读屏软件或长时间压测的验证；这些全部进入第 5 节 backlog。

### MVP 范围

- bundle/manifest 加载与验签占位。
- 编译器 MVP：TSX/TS/JS 函数组件 → 原生渲染调用；Oxc 去类型并降 JSX，rolldown 链接成单 ESM。信号图在 Rust；不做 VDOM、模板 IR 或预编译子树。
- 打包：`bubble bundle` / `bubble dev`；`bubble-shell` / `bubble:rt` 由宿主提供；资源按 `assets/<name>.<hash8>.<ext>` 落盘；Web 开发态 HMR。详见 [07-js-bundling-plan.md](07-js-bundling-plan.md)。
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

## 4. 里程碑

### M0 · Web 核心闭环（已实现）

范围：

- bundle/manifest 加载与验签占位。
- 编译器 MVP：TSX 函数组件 → 原生渲染调用。M0 的历史实现是单文件直译；打包重设计按 [07-js-bundling-plan.md](07-js-bundling-plan.md) 迁移到 Oxc + rolldown 单 ESM。信号图在 Rust。不做 VDOM、模板 IR 或预编译子树。
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

- 手势与滚动协商（[05-gestures-plan.md](05-gestures-plan.md)）。
- 自绘输入模型、Android IME、iOS `UITextInput`、Web 隐藏 textarea（[06-input-model-plan.md](06-input-model-plan.md)）。
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

M3 在 MVP 之后启动；其故障演练、性能门槛和更新回滚验证同样按第 5 节延后执行。

## 5. 延后验证 backlog

| 项目 | 触发时机 | 说明 |
|---|---|---|
| 实体设备矩阵 | post-MVP / 发布准备 | iOS/Android 真机、厂商键盘、外接键盘 |
| 真实读屏验收 | post-MVP / 发布准备 | VoiceOver、TalkBack、NVDA 核心路径 |
| 性能门槛 | post-MVP / M3 | 首帧、60fps、内存、包体、复杂列表 |
| 更新故障演练 | post-MVP / M3 | 坏签名、坏哈希、回滚、断网、LKG |
| golden / fuzz / conformance | post-MVP / 发布准备 | 布局与绘制快照、解析器/op fuzz、三端用例 |
| 跨端验收矩阵 | post-MVP / 发布准备 | 不再把平台差异写成当前里程碑门槛 |

这些项目只记录在本节，不进入当前里程碑的验收、代码路径或日常命令。

## 6. 测试策略（MVP）

- **Rust 进程内测试**：核心行为、状态不变量、边界与错误路径；确定性、快速、离线。
- **Node 单测**：Web 的 DOM/输入映射与键盘路由。
- **模拟器 smoke**：手动启动示例应用，完成一次点击/滚动/输入。现有 `bubble-eval` harness 可选用，不作为发布门槛。
- **回归测试**：只为用户报告或明确需求增加一个行为测试；不复制同一路径，不测 wiring、转发、源码文本或偶然默认值。
- **禁止**：真实设备/读屏自动化、新增 XCUITest/UiAutomation/Appium 套件、验收报告、截图/golden、fuzz、性能/FPS 门禁、只有验证用途的 crate 或 helper。

任何需要新验证工具、设备 harness、测试矩阵或跨平台验收的任务，先暂停并询问用户；默认答案是否。

## 7. 待定项

- 能力权限模型（manifest 允许列表 + 宿主授权）。
- 多实例与共享 VM/context。
- 差量更新、页面级独立包。
- grid/更完整 CSS。
- 宿主原生组件嵌入。
- 统一调试器与 QuickJS debug 适配。
- 桌面端（同一 Rust core + 平台 painter）。
- Web 主线程脚本无法硬终止。已接受，不单列方案。
- 超时/内存配额的最终数值。
- Web 带凭证请求的宿主代理协议。
- App Store 合规结论与 data-only 降级的产品边界。

## 8. 参考

- Compose Multiplatform 1.11 iOS 文本输入：`UIView` + `UITextInput`/`UIKeyInput` 代理。
  <https://kotlinlang.org/docs/multiplatform/whats-new-compose-111.html>
- JavaScriptCore 公有 API 无执行超时；`JSContextGroupSetExecutionTimeLimit` 属私有头文件。
  <https://stackoverflow.com/questions/21750816/is-there-any-way-stopping-or-pausing-the-execution-of-javascript-in-a-jscontext>
- QuickJS `JS_SetInterruptHandler` 执行中断。
  <https://bellard.org/quickjs/quickjs.html>
