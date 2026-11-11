# 04 · 风险与路线

## 1. 风险

| 风险 | 级别 | 影响 | 缓解 |
|---|---|---|---|
| QuickJS 事件循环、async、调试器需自建 | 高 | 开发效率与线上排障 | 基于 quickjs-ng/libc；先做最小事件循环；后续接调试协议；三端 conformance |
| Web Worker + Canvas2D + ARIA 覆盖层组合复杂 | 高 | Web 端体验与性能 | 最早做端到端原型；真实读屏/IME 设备验收；保留主线程渲染降级路径 |
| a11y/IME 是最大平台差异面 | 高 | 产品可用性与合规 | 每端独立适配层；VoiceOver/TalkBack/NVDA 自动化 + 手工回归 |
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
- 编译器 MVP：TSX → template IR + JS。
- Rust 保留树 + Taffy 布局。
- op 协议 + 双缓冲提交。
- Web Worker 脚本 + Canvas2D painter + 事件命中。

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

### M2 · 输入与无障碍

范围：

- UI 线程编辑状态 + 三端输入代理。
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

- **conformance**：ES 子集在 QuickJS 与 Chromium/Safari/Firefox Worker 上跑同一套用例；统一 polyfill。
- **golden**：template IR、布局树、三端绘制快照；文本差异单独标注。
- **fuzz**：manifest/parser、op decoder、模板 IR 校验器。
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
- 编译器实现语言（Rust/oxc vs Node/Babel）与类型检查链路。
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
