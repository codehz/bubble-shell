# 04 · 风险与路线

## 1. 风险

| 风险 | 级别 | 影响 | 缓解 |
|---|---|---|---|
| QuickJS 事件循环、async、调试器需自建 | 高 | 开发效率与线上排障 | 基于 quickjs-ng/libc；先做最小事件循环；后续接调试协议；三端 conformance |
| 主线程脚本卡住绘制 | 中 | Web 长任务掉帧，且无法强杀 | 接受。软预算只埋点；不做 Worker、不做切片 |
| a11y/IME 是最大平台差异面 | 高 | 产品可用性与合规 | 每端独立适配层；VoiceOver/TalkBack/NVDA 自动化 + 手工回归 |
| Android IME 与自绘编辑状态语义不一致 | 高 | CJK 组合、选区、删除与光标位置失败 | 按 [自绘输入模型重设计计划](06-input-model-plan.md) 锁步迁移编辑事务；模拟器验证；真机差异记入发布记录 |
| 全表面重绘性能 | 中 | 60fps 门槛 | 脏矩形、静态子树缓存、帧耗时埋点；复杂列表虚拟化 |
| 远程 JS 的 App Store 合规 | 高 | iOS 上线 | 第一方签名、无 JIT；准备 data-only 降级；提前审核沟通 |
| FFI / QuickJS 内存安全 | 高 | 宿主崩溃 | 安全封装、fuzz、panic 屏障、配额、灰度 |
| 三端文本布局差异（已接受） | 中 | 视觉/高度不一致 | 每端 golden 快照；布局不依赖固定文本高度 |
| 密钥管理与回滚 | 中 | 供应链安全 | `keyId` 轮换、吊销、单调版本、LKG、回退演练 |
| Web CORS / 鉴权 | 中 | 数据获取受限 | 公开内容 runtime fetch；带凭证 API 走宿主代理 |
| QuickJS 性能低于 JIT 引擎 | 中 | 复杂脚本卡顿 | 事件粒度小、重计算下沉宿主、性能基线 |

## 2. v1 明确不做

- 多实例、差量更新、grid/完整 CSS、高级视觉效果。
- 宿主原生组件嵌入、能力授权 UI、统一调试器、桌面端。
- 跨端像素级一致；字体内置；运行时自带字体。

## 3. 里程碑

### M0 · Web 核心闭环

范围：

- bundle/manifest 加载与验签占位。
- 编译器 MVP：TSX 函数组件 → 原生渲染调用（Oxc）。信号图在 Rust。不做 VDOM、模板 IR 或预编译子树。
- Rust 保留树 + Taffy 布局。
- op 协议 + 双缓冲提交。
- Web：主线程一份 WASM + JS 回放 `PaintCmd`。无 Worker。

验收：

- 示例 bundle 能渲染、点击更新、滚动。
- 简单页面稳定 60fps；首帧 <400ms（中端笔记本）。
- op 批次可重放、可断言。

### M1 · Native 核心

范围：

- QuickJS 集成、C ABI、事件循环、超时/内存限制。
- iOS CoreGraphics、Android Canvas painter。
- 宿主能力桥、单实例嵌入、生命周期。
- 宿主注入字体。

验收：

- 同一 bundle 在 iOS/Android 渲染与交互。
- 脚本死循环能被中断并恢复，宿主 UI 不卡。
- 简单页面 60fps。

#### M1 本机验证记录（2026-11-11）

按测试职责拆分 `examples/app/fixtures/*.tsx`，由 `examples/build-fixtures.sh` 生成 `examples/web/fixtures/<name>.{js,json}`。`counter` 用于点击/滚动/中断，`editing` 用于文本输入，`gestures` 用于手势；同一原生宿主按资产名选择 fixture，不再把所有内容塞进一个入口。两端的设备内驱动完成「点击 → `Count: 1` → 注入死循环 → 1s 中断并重挂载 → 断言 `Count: 0`」流程。

| 环境 | 配置 |
|---|---|
| 主机 | Apple M2，macOS 27.0.0 arm64 |
| iOS | iPhone 18 Pro 模拟器，iOS 27.2，arm64 iOS 17.0 模拟器构建 |
| Android | `sdk_phone64_arm64` 模拟器，Android 16（SDK 36），arm64-v8a，1080×2400 / 420dpi / 60Hz，Metal 硬件加速 |

| 检查 | iOS | Android |
|---|---|---|
| remount text | `Count: 0` | `Count: 0` |
| faults | 1 | 1 |
| frames during hang | 58 | 60 |
| frame budget | 16.87 ms | 16.67 ms |
| font epoch | 1 | 1 |
| scroll offset | 41.7 | 47.7 |
| paint bytes | 1652 | 1652 |
| 设备内耗时（启动→报告） | ≈4460 ms | ≈1490 ms |

复现命令：

```sh
cargo run -q -p bubble-eval -- ios
cargo run -q -p bubble-eval -- android
```

说明：

- Android 硬件加速前后差异明显：SwiftShader 软件渲染时 frame budget 为 38.76 / 35.32 ms、死循环期间仅 30–31 帧；切换为 Metal 后为 16.65 / 16.67 ms、61 帧。`dumpsys gfxinfo` 显示 82 帧中 2 帧 janky（2.44%），`Missed Vsync` 为 0。此前的不达标是模拟器软件光栅化导致，不是运行时缺陷。
- iOS 已接入 `UIPanGestureRecognizer` → `bubble_scroll`，Android 已接入 touch move → `bubble_scroll`；两端均同时传递水平与垂直增量。滚动验收使用真实合成触摸：iOS 由 `XCUITest` 拖拽（`examples/ios/BubbleShellUITests.xcodeproj`），Android 由 `adb shell input swipe` 在 scroller 屏幕矩形内滑动；demo bundle 扩展到 48 行，保证在真机高度下也可滚动。两端 scroll offset 均超过 40。
- 真机 60fps 基线待补测；宿主桥与生命周期暂停由 headless 套件覆盖。
- `crates/native/tests/m1.rs` 已用进程级互斥串行化，`cargo test --workspace` 默认并行运行通过。内存超限与硬超时走同一重建路径，测试覆盖内存超限、双实例拒绝与坏 manifest 拒绝。
- iOS 设备脚本现在会删除上次的 `m1-result.json` 再启动，避免读到旧报告。

### M2 · 输入与无障碍

前置阶段：[手势与滚动协商计划](05-gestures-plan.md) 已实现，两端模拟器自动化覆盖主要交互；iOS 双指同向拖、第三指与按住中断待真机补充。[自绘输入模型重设计计划](06-input-model-plan.md) 的 A–F 已完成：编辑事务、Android IME 语义、三端排版几何和输入模型验收矩阵均落地，验收入口为 `cargo run -q -p bubble-eval -- editing-{web,android,ios}`。

M2 实现进展（2026-11-12）：核心已具备焦点树、Tab/Shift-Tab、Enter/Space 激活、Esc、Left/Right/Home/End、`modal` 焦点陷阱；语义树由 `role`/`label`/`disabled`/`hidden`/`checked`/`selected`/`expanded` 与节点类型派生，支持 `focus`/`activate`/`setValue` action。Web 使用透明 ARIA 覆盖层与 canvas 键盘入口；Android 使用 `AccessibilityNodeProvider`；iOS 使用 `UIAccessibilityElement`。三端均绘制选区手柄并支持拖动；Web/Android/iOS 编辑菜单分别走核心 `edit_menu`、系统 IME 菜单与 `UIMenuController`。读屏滚动 action 已收归核心：语义节点输出 forward/backward/left/right/up/down 位，Android `AccessibilityNodeProvider` 与 iOS `accessibilityScroll` 映射到同一 action，滚动目标为最近的可滚动祖先，步长由核心按可见范围计算。`cargo test --workspace`、`cargo run -q -p bubble-eval -- editing-web` 与三端构建通过；`editing-android` 新增 8 项真实 provider 检查并达到 82 项通过；Web 已人工验证 Tab 焦点、拖动选区、手柄和右键菜单。

M2 Web 滚动衔接（2026-11-12）：ARIA 覆盖层已把聚焦节点的 PageDown/PageUp 与方向键路由到核心滚动 action，沿语义父链检查各轴支持能力；静态文本与滚动容器保持 DOM 焦点，文本框保留编辑按键，修饰键与 composition 不触发滚动。覆盖层也接入 Tab/Shift-Tab 与激活后的 IME 切换。`editing-web` 现含 9 项 IME / 无障碍按键验收；Chromium 实际页面验证 96px 容器每页约 76.8px、底部钳制、反向返回、焦点保留、Tab 转入输入框及 Enter/Esc 编辑切换。

M2 弹层焦点隔离（2026-11-12）：核心统一限制直接 focus、指针字段聚焦与读屏 action，拒绝隐藏/禁用目标和活动 modal 外节点；背景不发布 action，弹层内读屏滚动不会落到背景祖先。活动 modal 按可见深度选择，隐藏祖先下的 modal 不再锁住 Tab；op 批次使原焦点不可用时清除焦点，失败的 focus 不丢弃当前编辑批次。活动弹层输出为独立语义根，Web ARIA DOM 按 parent 嵌套并设置 `aria-modal`，键盘焦点从 IME 切回按钮时同步 DOM，关闭弹层后回到 canvas。新增共享 `focus` fixture（Web：`?fixture=focus`）覆盖外层/嵌套弹层。Chromium 实际页面已验证 Enter 开启、Tab/Shift-Tab 循环、IME/按钮切换、Esc 清焦点、嵌套关闭和关闭后背景恢复可达；核心 116 项、native 7 项、Android 模拟器 82 项、iOS 模拟器 27 项既有输入验收通过。Android provider 同步返回核心 action 拒绝结果并清除隐藏节点的读屏焦点。

M2 尚未完成的部分是设备级读屏验收与平台差异收尾：VoiceOver / TalkBack / NVDA 的核心路径尚未自动化回归，实体设备与厂商键盘差异未测；Android/iOS 弹层焦点陷阱的设备级交互测试待补。Web 本轮浏览器验收覆盖 DOM 焦点、可访问树与核心 action，尚不等于真实读屏软件验收。

M2 范围：

- 在 UI 线程编辑会话上补齐输入控件行为：原生选区手柄、编辑菜单、触摸拖动选区。
- iOS/Android 原生手柄/菜单；Web 自绘。
- Rust 语义树 + 平台 a11y 桥 + Web ARIA 覆盖层。
- 焦点树、键盘导航、读屏 action。

验收：

- VoiceOver / TalkBack / NVDA 完成核心路径。
- CJK IME、选区、composition、受控值 epoch 正常。
- 键盘 Tab/Enter/Esc、弹层焦点陷阱正常。

### M3 · 更新、观测与性能

范围：

- Ed25519 manifest、内容寻址、LKG、宿主激活、自动回退。
- 灰度、kill switch、兼容性拒绝。
- 结构化埋点、帧统计、错误上报。
- 性能加固与包体优化。

验收：

- 坏签名/坏哈希/回滚/断网演练通过。
- 达到总览中的验收指标。
- Web 包 <3MB gzip；native 内存增量 <100MB。

## 4. 测试策略

- **conformance**：ES 子集在 QuickJS 与浏览器主线程上跑同一套用例；统一 polyfill。
- **golden**：布局树、三端绘制快照；文本差异单独标注。
- **fuzz**：manifest/parser、op decoder。
- **a11y**：语义树快照、焦点顺序、读屏设备回归。
- **update**：坏签名、坏哈希、版本回放、挂载失败回退、kill switch。
- **perf**：启动、首帧、帧耗时、内存、包体；真机 CI 基线。

## 5. 待定项

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

## 6. 参考

- Compose Multiplatform 1.11 iOS 文本输入：`UIView` + `UITextInput`/`UIKeyInput` 代理。
  <https://kotlinlang.org/docs/multiplatform/whats-new-compose-111.html>
- JavaScriptCore 公有 API 无执行超时；`JSContextGroupSetExecutionTimeLimit` 属私有头文件。
  <https://stackoverflow.com/questions/21750816/is-there-any-way-stopping-or-pausing-the-execution-of-javascript-in-a-jscontext>
- QuickJS `JS_SetInterruptHandler` 执行中断。
  <https://bellard.org/quickjs/quickjs.html>
