# 06 · 自绘输入模型重设计计划

## 1. 目标与范围

重做文本编辑内核和平台输入协议，修复 Android IME 桥接的语义缺口，同时保留完全自绘的文本、光标和选区。Android `InputConnection` 只接入系统 IME、手柄、菜单及自动填充；不使用 `EditText` 或其他原生文本视图作为编辑器。

本计划先支持可靠的纯文本编辑，但建立可扩展到富文本的文档位置、范围事务和投影边界。首期不实现样式段、富文本工具栏、内联对象、rope/片段树或富文本 UI。

在当前 M2 前置计划 [05-gestures-plan.md](05-gestures-plan.md) 的阶段 E 验收完成后，本计划是下一个开发计划；其后继续 M2 的焦点、无障碍和输入控件收尾工作。阶段 E 的两端模拟器自动化已覆盖支持范围，但 iOS 公共 XCUITest 无法表达任意双指轨迹、第三指和按住期间系统中断，因此仍不得视为完整通过。

## 2. 源码现状与问题

- `crates/core/src/editing.rs` 以 UTF-16 单位记录选区和组合区，但组合更新把光标固定到组合文本末尾；组合期间拒绝选区、普通插入和删除。
- `examples/android/app/src/main/java/dev/bubble/shell/BubbleView.kt` 的 `InputConnection` 忽略 `newCursorPosition`，并把 `deleteSurroundingText` 转成重复退格。`composing` / `composingText` 在 Kotlin 和 Rust 两处维护，可能分叉。
- 当前几何计算 `text_metrics` / `hit_unit` 根据显式换行和前缀测宽推导位置；Android 绘制使用 `StaticLayout`。自动换行、双向文本下测量、命中、绘制和 IME 坐标可能不一致。
- `BSED` v1 每个编辑事件携带完整字符串；Android 单命令逐次调用会导致同一 IME batch 多次桥接和刷新。
- iOS `UITextInput`、Web 隐藏 `textarea` 各自适配现有内核。iOS 标记文本内部选择同样固定在末尾；三端必须一起迁移，避免保留两套编辑语义。

## 3. 设计决定

### 3.1 权威状态和会话

Rust UI 线程拥有唯一权威文档及编辑状态：文档、方向性选区、组合范围、revision、撤销历史和活动编辑会话。平台适配器通过会话读取原子快照并提交编辑事务；Android 不维护第二份业务文档。若 Android API 需要 `Editable` 形状的互操作对象，它只能是会话投影，不能独立接受并缓存最终状态。

会话有明确的开始、关闭和失效语义。焦点切换、字段销毁、宿主失焦/暂停、输入连接关闭及 runtime 重建都会使旧会话失效；旧连接不得把输入应用到新字段或新 generation。每次提交校验会话身份和期望 revision，并返回成功、冲突或失效，不静默丢失命令。

### 3.2 文档位置与编辑事务

首期继续使用 Rust `String`。对外定义可扩展的文档位置与范围事务，而不把 Android 的 `Int` 索引泄漏为核心模型：

- 平台 IME 边界使用 UTF-16 code unit offset；转换到文档位置时校验边界，不能把代理对中间位置拆成非法字符。
- 选区保存 anchor 与 active 端点，保留方向；光标是折叠选区。
- 组合范围独立于选区；选区允许位于组合范围内，IME 可在组合过程中移动、替换和删除。
- 事务表达带范围的替换、设置选区、设置/更新/结束组合、删除范围、撤销/重做及显式 reset。替换正确应用 IME 提供的 `newCursorPosition`。
- 同步编辑规则（单行、maxlength 等）在 UI 线程事务内执行。脚本不参与逐键往返。
- 文档位置预留 affinity，使后续接入多段落、样式区间或原子内容时可定义插入点在边界的归属；不在本轮实现富文本内容结构。

### 3.3 组合、应用写入与 revision

IME 命令是当前活动组合的唯一编辑者。普通应用 `SetEditingValue` 在组合期间返回明确的 composition conflict，不应用、不延后、不静默忽略。应用若确需接管，必须显式 reset：结束并取消当前组合、使相关旧 IME 快照失效，再按 revision 条件应用新值。

每个已提交状态变更递增单调 revision。脚本写入必须携带其观察到的 epoch：过期写入返回 stale-revision conflict；不得覆盖较新的 UI 编辑。冲突类型和结果应可诊断，但日志不得包含用户输入内容。IME 连接只在确实需要重新同步时收到状态更新，避免每个回调触发无条件 `restartInput`。

### 3.4 原子事务与脚本事件

编辑会话提供嵌套 `beginBatchEdit` / `endBatchEdit`。批次中的读取反映 UI 线程当前事务状态；批次结束原子提交、更新 revision、刷新布局，并向脚本发布一个带 revision 的编辑 delta 批次和最终选区/组合区快照。没有显式批次的单个命令等价于单命令事务。错误或关闭时必须平衡事务深度并丢弃未提交批次，不能留下半应用编辑状态。

新事件契约采用有序 delta 加最终状态：delta 描述被替换范围、插入内容以及必要的选择/组合变化；最终快照用于同步和冲突诊断。批次内多次修改不得复制或发送每步完整文档。密码字段的事件和诊断继续遵守敏感内容不记录规则；向脚本公开的字段值沿用现有受控输入契约。

协议版本锁步升级并迁移所有生产者和消费者，不增加旧 BSED 兼容垫片或运行时协商。更新 Rust、native C ABI/JNI、WASM、Kotlin、Swift、Web runtime/host 和相关测试；删除旧命令路径及已被替代的状态。

### 3.5 平台排版快照

平台排版适配器是实际字形整形和行布局的权威来源。每次布局为文本字段生成与当前文档 revision、字体/样式和约束绑定的 `LayoutSnapshot`。快照提供 UTF-16 文本边界到以下几何数据的映射：插入符矩形、字符边界、选区矩形、行区间及方向信息。

- Android 使用与 Canvas 绘制相同的 `StaticLayout`/平台布局结果；不能分别用一个 Layout 绘制、再由 Rust 前缀宽度估计 IME 位置。
- iOS 使用与 CoreGraphics 文本绘制相同的行布局结果；`UITextInput` 的 caret、selection、firstRect、命中和 tokenizer 查询共享同一映射。
- Web 使用同一次 Canvas 文本布局/测量结果驱动自绘选区、光标命中和隐藏 `textarea` 定位；composition 仍由浏览器代理承接。
- 快照失效键至少包含文本 revision、可用宽度、字体/字号/字重/行高、对齐和密度变换。旧 revision 的几何不可用于当前 IME 更新。
- bidi 选区可由多个矩形组成；不能用逻辑前缀宽度假设视觉位置单调。

平台之间接受字体和换行差异；每个平台内部绘制、命中、选择与 IME 几何必须一致。

## 4. 实施阶段与顺序

阶段依赖：先完成 `05-gestures-plan.md` 阶段 E；然后按 A→F 实施。每个阶段完成后再进入下一阶段，协议切换使用一次锁步迁移，不部署双轨兼容。

| 阶段 | 范围 | 交付 |
|---|---|---|
| A. 状态与事务契约 | 文档位置、范围替换、方向性选区、组合区独立、session/revision/conflict、嵌套原子事务 | Rust 可独立测试的编辑会话与事务语义；边界/错误行为固定 |
| B. 增量事件与 ABI | delta + 最终快照、批次发布、版本锁步迁移 | BSED 新契约；C ABI/JNI/WASM 均以事务读写同一 UI 状态 |
| C. Android InputConnection | 面向活动会话实现读写；支持 batch edit、newCursorPosition、组合区域/内部选区、周边文本/选择、代码点删除、连接关闭与失效 | 去除 Kotlin 重复 composition 状态和重复退格实现；保留键盘配置、autofill、CursorAnchorInfo、原生手柄/菜单 |
| D. 平台适配迁移 | iOS `UITextInput`、Web textarea 事件映射和 host 同步迁移 | 三端遵循相同事务、冲突、组合与 revision 规则；移除旧行为 |
| E. 几何快照 | 三端平台布局快照、版本失效、光标/选区/命中/绘制共用 | 自动换行、组合文本和 bidi 的内部几何一致；淘汰旧前缀测宽的编辑几何路径 |
| F. 集成验收和文档切换 | 跨端契约、模拟器矩阵、文档/示例、故障定位 | 输入文档描述实际新行为；旧协议和旧桥接说明删除 |

可并行实现的阶段内平台工作应共享 A/B 固定的 ABI 与 `LayoutSnapshot` 契约；不能各自发明平台私有编辑语义。

## 5. 验收矩阵

| 层面 | 必须证明的行为 |
|---|---|
| Rust 文档边界 | UTF-16 代理对边界、反向选区、空文本/文末、范围替换、maxlength、单行过滤、方向 affinity |
| Composition | CJK 开始/更新/确认/取消；确认后只产生一次组合历史事务；组合区与 selection 独立；组合内选择、删除、替换后状态正确 |
| Android IME | `newCursorPosition` 正负和边界；`deleteSurroundingText` 与 codepoint 变体；setComposingRegion；嵌套 batch；getTextBefore/After、getSelectedText、getExtractedText；restart/close/失焦后的旧连接拒绝写入；CursorAnchorInfo 更新 |
| 输入与冲突 | 多次 IME 操作一批一次事件；应用普通写入返回 composing conflict；显式 reset 后旧 IME 输入失效；过期 epoch 返回 stale conflict；错误路径无部分提交或事务泄漏 |
| iOS | `setMarkedText` 的内部选区准确；replace range/selection/caretRect/selectionRects/firstRect 与当前 session 一致；旧输入代理失效 |
| Web | beforeinput 与 composition 不双重应用；浏览器代理 selection 与 Rust 状态回读一致；字段切换、reset、过期事件处理正确 |
| 几何 | 每个平台同一布局结果驱动绘制、点击 caret、拖动选择及 IME 坐标；覆盖自动换行、空文本、非 BMP 字符、混合 RTL/LTR、组合区和布局宽度变化 |
| 事件协议 | delta 顺序可重放到最终快照；批次 revision 单调；旧协议版本按锁步发布，不留兼容路径；所有生产者/消费者同步升级 |
| 安全与诊断 | 无敏感文本日志；异常/失效会话可诊断且不会写错字段或 generation |

### 发布门槛

必须通过 Rust 核心/协议测试、Android 模拟器输入自动化，以及 iOS/Web 对应的目标平台检查。Android 模拟器测试至少覆盖 CJK IME 组合中移动、删除、确认与选区手柄；记录模拟器 API、系统镜像和键盘版本。实体 Android 设备验收明确延后，不是本计划的阻塞门槛；因此发布记录必须声明真实设备/键盘差异仍有残余风险。

## 6. 非目标与风险

- 本轮不交付富文本内容结构、样式片段编辑、嵌入节点、链接/提及 UI 或富文本工具栏。事务、位置 affinity 和几何投影负责避免未来被原生文本框锁死，不承诺富文本无需继续扩展协议。
- 不引入新的文本 shaping 引擎；平台排版仍允许跨端差异。
- 不以隐藏 `EditText` 作为 Android 后备实现。
- Android SDK 最低版本在当前仓库中未找到明确配置；开始 C 阶段前必须从实际 Gradle/build 配置确认支持 API 范围，并据此选用受支持的 `InputConnection` 方法。
- 模拟器通过不等价于各厂商键盘/实体设备兼容。真实设备实测明确延期，特别注意 Samsung、Gboard、CJK 和手写输入差异。
- 事件 delta 减少全量字符串复制，但增加重放与最终快照一致性的协议责任；必须用属性测试或确定性重放测试证明任意批次后状态一致。

## 7. 参考方案

- Android 官方 `InputConnection`：编辑器可自行实现；组合区与选区独立，并应测试多种 IME 和 CJK/RTL。
  <https://developer.android.com/reference/android/view/inputmethod/InputConnection>
- Jetpack Compose `StatelessInputConnection`：连接委托给活动 `TextInputSession`，输入命令在 session 中处理，支持批处理。
  <https://github.com/androidx/androidx/blob/androidx-main/compose/foundation/foundation/src/androidMain/kotlin/androidx/compose/foundation/text/input/internal/StatelessInputConnection.android.kt>
- Flutter `InputConnectionAdaptor` / `ListenableEditingState`：观察编辑状态并合并 batch edit 通知。
  <https://github.com/flutter/flutter/blob/master/engine/src/flutter/shell/platform/android/io/flutter/plugin/editing/InputConnectionAdaptor.java>
  <https://github.com/flutter/flutter/blob/master/engine/src/flutter/shell/platform/android/io/flutter/plugin/editing/ListenableEditingState.java>
