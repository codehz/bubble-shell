# 03 · 嵌入、输入、更新

## 1. 嵌入模型

v1 为**单实例**：每个宿主进程一个 runtime、一个内容根、一个版本。

宿主负责：

- 创建并持有 surface（iOS `UIView`、Android `View`、Web canvas 容器）。
- 传入尺寸、safe area / keyboard insets、像素比。
- 注册能力实现（fetch、storage、navigation、clipboard 等）。
- 注入字体。
- 处理 navigation intent、返回手势、深链。
- 转发前后台、可见性、内存警告等生命周期。

runtime 负责：

- 加载、验签、激活 bundle；维护 LKG。
- 脚本线程与 UI 线程。
- 保留树、布局、绘制、动画、命中、滚动。
- 语义树、输入代理、编辑状态。
- 更新缓存、配额、错误上报。

### 宿主 API 草图

平台中立的伪代码，最终以各端 SDK 为准：

```ts
interface ShellHost {
  surface: SurfaceHandle;
  insets: Insets;
  capabilities: CapabilityRegistry;
  navigate(intent: NavigationIntent): void;
}

const shell = createShell({
  host,
  bundle: { url, publicKeys },
});

shell.setFrame({ x, y, width, height });
shell.setVisible(true);
shell.destroy();
```

### 生命周期

- `setVisible(false)`：暂停 vsync、动画、滚动惯性；保留树与状态。
- 进入后台：同上；网络与脚本定时器按宿主策略暂停。
- 内存警告：清图片缓存、文本缓存、非活动资源；保留树与当前 bundle。
- `destroy()`：停止脚本线程、释放 surface、关闭缓存句柄。
- 安全区/键盘 insets 变化：作为布局输入，不触发脚本重渲染。

## 2. 宿主能力桥

所有能力异步：

```text
Request  { id, method, params, timeoutMs }
Response { id, ok, result | error }
Cancel   { id }
```

- 每个请求带超时与取消；错误用稳定错误码，不抛宿主异常到脚本。
- 能力注册表允许宿主覆盖默认实现；同一 method 只有一个实现。
- v1 权限模型为 ambient：脚本可直接调用已注册能力。
- 后续在桥接层加 manifest 能力声明 + 宿主策略；脚本 API 不变。

### v1 能力

| 能力 | API | 说明 |
|---|---|---|
| fetch | `fetch(url, init)` | runtime 缓存/离线；Web 受 CORS 限制 |
| storage | `get/set/remove/keys` | 按 bundle 命名空间隔离；配额 |
| navigation | `navigate(intent)` | 只发 intent，宿主拥有路由 |
| clipboard | `readText/writeText` | Web 经主线程代理 |
| ui | `toast/openURL` | 宿主实现 |
| lifecycle | 事件订阅 | 前后台、insets、可见性 |
| fonts | `registerFont(family, data)` | 宿主注入；递增 fontEpoch |
| haptics | 可选 | 宿主实现 |

### 跨线程代理

- Native：脚本线程 → C ABI → UI 线程/宿主线程；结果回调脚本线程。
- Web：同线程调用宿主 API，结果用 Promise 回到这次事件循环。没有 `postMessage`。
- 不做同步调用；脚本需要布局结果时读上一帧缓存或 await。

## 3. 导航

- runtime 只发 navigation intent：`push`、`pop`、`replace`、`openURL`、`back`。
- 宿主拥有路由、转场动画、返回栈、深链、权限与埋点。
- 内容内部多页也映射为宿主路由；runtime 不维护全局页面栈。
- 返回手势/返回键由宿主处理；runtime 可提供 `canGoBack` 状态供宿主查询。

## 4. 无障碍

语义树跟随 Rust 保留树，作为节点属性维护：

- `role`、`label`、`value`、`state`（disabled/checked/selected/expanded）、`bounds`、`hidden`、`focusable`、`actions`。
- 语义更新与 op 同批次提交，UI 线程每帧同步到平台。

平台映射：

- iOS：UIAccessibility container / elements；焦点与激活走语义树。
- Android：AccessibilityNodeProvider / ExploreByTouchHelper；虚拟节点树。
- Web：与画布同步的透明 ARIA DOM 覆盖层；Canvas 是可聚焦的 `role="application"` 键盘入口，覆盖层元素 `tabindex="-1"`，避免与 Rust 焦点顺序重复。

Web 覆盖层要求：

- 每个语义节点一个固定定位元素，bounds 与绘制同步；DOM 按语义 `parent` 嵌套，避免角色层级丢失。活动 modal 成为独立语义根，不能嵌在 hidden 背景下。
- 使用 `aria-label`、`aria-checked`、`aria-selected`、`aria-expanded`、`aria-disabled`、`aria-hidden`、`tabindex`；活动 modal 带 `aria-modal="true"`，未声明角色时映射为 `dialog`。
- 焦点环由核心绘制；键盘 Tab 顺序由 Rust 焦点树决定，不依赖 DOM 原生 Tab。
- 覆盖层聚焦节点支持 PageDown/PageUp 和方向键滚动：沿语义 `parent` 链检查对应 `actions` 位，再从当前节点调用核心 scroll action；步长与边界由核心处理，滚动后保留 DOM 焦点。文本框、组合输入与带修饰键的快捷键不走此滚动入口。静态节点聚焦时清除旧的核心键盘焦点；Tab/Shift-Tab 继续使用 Rust 焦点顺序，文本框激活后转入 IME。
- `aria-live` 用于 Toast/状态更新；原生端用 announcement API。

焦点与键盘：

- Rust 维护焦点树与 tab 顺序；`focusable`、交互 `role` 和 `TextField` 参与 Tab，`hidden`/`disabled` 节点跳过。
- Tab/Shift-Tab、Enter/Space、Esc、Left/Right/Home/End 在 UI 线程处理；Enter/Space 激活目标上的 `TapGesture`，Esc 清除焦点。
- 读屏 action 经 `bubble_a11y_action` / wasm `a11y_action` 路由到 Rust：`focus`、`activate`、`setValue`，以及 `scroll` 的 forward/backward/left/right/up/down。滚动 action 从收到的语义节点向上找第一个接受该轴的可滚动祖先，步长为可见范围的 80%，再走与触摸相同的 `scroll_path`；Android 依据 `actions` 位添加对应系统 action，iOS 把 `UIAccessibilityScrollDirection` 映射到同一组 action。无角色但可滚动的容器也进入语义树，平台不再自行计算像素增量。
- 最深的可见 `modal` 容器限制 Tab/Shift-Tab 顺序；同深度以后出现的节点优先，`hidden` 祖先下的 modal 不生效。背景语义标记为 hidden 且不发布 action，活动 modal 的语义 parent/depth 为 0/0。
- 直接 focus、指针字段聚焦与读屏 action 均拒绝 hidden、disabled 或活动 modal 外的目标；读屏滚动不能越过 modal 边界去滚动背景祖先。失败的聚焦请求不取消当前字段的编辑批次。
- 一个 op 批次完成后，若当前焦点已隐藏、禁用、移出活动 modal 或失去可聚焦能力，则清除焦点并中止未提交编辑批次。Esc 清除当前焦点，不关闭弹层；下一次 Tab/Shift-Tab 从活动 modal 的首/尾项继续。
- Web 的键盘焦点跟随 Rust：文本字段进入 IME 代理，其他控件进入 ARIA 元素；焦点目标移除或隐藏后回到 canvas。未打开的编辑菜单保持隐藏，不参与读屏导航。

## 5. 文本输入与 IME

编辑状态在 **UI 线程**，避免每个按键跨线程：

```text
EditSession {
  value, directional selection, composition range,
  composing, revision, undo history
}
```

平台输入代理：

| 平台 | 代理 | 当前接入 |
|---|---|---|
| iOS | `UIView` 实现 `UITextInput` / `UIKeyInput` | 光标、选区矩形、标记文本；自绘选区手柄与拖动、`UIMenuController` 编辑菜单、`UIAccessibilityElement` 语义树、Tab/Esc 硬件键 |
| Android | `InputConnection`（自定义 View） | `CursorAnchorInfo`、IME 组合与程序化选区；自绘手柄与拖动、`AccessibilityNodeProvider` 虚拟语义树、硬件 Tab/Esc |
| Web | 隐藏 `textarea`，定位到光标 | 自绘选区/手柄与拖动、核心菜单、透明 ARIA 覆盖层、canvas 键盘事件 |

数据流：

1. 用户输入 → 平台代理 → UI 线程编辑状态。
2. UI 线程绘制文本、光标、选区。
3. UI 线程向脚本异步发送 `EditingChanged`。
4. 脚本可下发 `SetEditingValue(handle, epoch, ...)`。
5. epoch 落后则忽略；composition 期间以 UI 线程为准。

覆盖能力：键盘类型、autofill、密码、多行、maxlength、输入过滤、撤销/重做。三端行为差异记录在测试矩阵中。

测试入口按职责拆分：`examples/build-fixtures.sh` 编译 `examples/app/fixtures/*.tsx` 并生成 `examples/web/fixtures/<name>.{js,json}`；Web 用 `?fixture=counter|editing|gestures` 选择，Android 用 `bubble_fixture` intent extra，iOS 用 `BUBBLE_FIXTURE` 环境变量。原生桥只实现一次，设备测试只选择对应 bundle，不复制入口。

当前接入：Web 隐藏 `textarea` 把 `beforeinput` / composition 映射为事务命令（commit/compose/delete/replace/select/undo/redo，`examples/web/ime.js`），宿主按快照的 session 提交并回读选区同步代理，含 `historyUndo` / `historyRedo`。在字段上按下并拖动会改选区，不再交给手势；抬起不把插入符折回落点。选区手柄由核心给出几何并由宿主绘制；拖动手柄保持另一端锚点。右键菜单调用核心 `edit_menu` 执行剪切、复制、粘贴和全选，密码字段不复制。iOS `UITextInput` 的 `insertText` / `deleteBackward` / `replace` / `setMarkedText` / `unmarkText` 通过 `bubble_edit_snapshot` 读取 session，再走 commit/compose/replace/select/batch，标记文本内部选区不再固定到末尾。Android `InputConnection` 绑定创建时的 `EditSession`，每次读写校验 session；`commitText` / `setComposingText` 走范围事务并应用 `newCursorPosition`，支持内部组合选区、`setComposingRegion`、UTF-16 与代码点两种 `deleteSurroundingText`、嵌套 batch edit、`getTextBefore/After`、`getSelectedText`、`getExtractedText` 和 `CursorAnchorInfo`。连接关闭或字段切换后旧 session 写入被拒绝，Kotlin 不再维护第二份 composition 状态。iOS `UITextInput` 覆盖插入、退格、UTF-16 选区、标记文本（`setMarkedText` / `unmarkText`）和选区矩形；⌘Z / ⇧⌘Z 走同一撤销栈。标记文本内部的选中范围仍固定在组合末尾。点按 `focus_at` 聚焦并把插入符落到最近的 UTF-16 边界。聚焦字段在 layout 后绘制选区与 1px 光标。撤销按字段保留最近 100 步；脚本 `SetEditingValue` 成功后清空该字段的撤销栈。`field_flags` 的 bit0 多行、bit1 密码，bits 8..15 为键盘种类：1 number、2 email、3 url、4 phone。bits 16..23 为 `autofill`：1 username、2 password、3 newPassword、4 email、5 phone、6 oneTimeCode。

Web 宿主从 `runtime.js` 具名导入 `setApi`、`dispatchGestureEvents` 和 `dispatchEditEvents`；编辑事件由 `dispatchEditEvents` 解码并调用字段的 `onChange`。隐藏 `textarea` 通过 `inputMode` 与 `autocomplete` 配置键盘和自动填充，不写入只读的 `type` 属性；密码遮罩由核心绘制。

点击 Web 文本框时，`placeCaret` 在布局成功后立即重绘 Canvas，再定位并聚焦隐藏输入代理；首次点击和切换字段都立即显示光标，无需等待输入或手势动画触发重绘。

三端已实现行为的差异：

| 行为 | 核心 | Web | Android | iOS |
|---|---|---|---|---|
| 单行换行 | 去掉 `\n` / `\r` | 同核心 | 同核心 | 同核心 |
| 多行回车 | 去掉 `\r`，保留 `\n` | 同核心 | 同核心 | 同核心 |
| composition | 独立组合区；未 `cancel` 时忽略脚本写入 | `composition*` 事件；`insertCompositionText` 丢弃 | session 内组合区；`setComposingText` / `setComposingRegion` / 内部选区 | `setMarkedText` / `unmarkText`；标记文本内部选区可独立设置 |
| 撤销 | 每字段 100 步；脚本值清空 | `historyUndo` / `historyRedo` | 未接按键 | ⌘Z / ⇧⌘Z |
| 拖动选区 | `SetSelection`；选区手柄几何 | 字段拖动与手柄拖动 | 字段拖动与手柄拖动 | 字段拖动与手柄拖动 |
| 菜单 | `edit_menu` | 核心菜单 | 系统菜单，依赖 IME | `UIMenuController` 标准编辑动作 |
| 光标位置 | layout 后的 `caret_box` | 隐藏 `textarea` 贴到光标 | `CursorAnchorInfo` | `caretRect(for:)` |
| autofill | `field_flags` bits 16..23 | `autocomplete` | autofill hints | `textContentType` |

输入模型验收入口：

```sh
cargo test --workspace
cargo run -q -p bubble-eval -- editing-web
cargo run -q -p bubble-eval -- editing-android
cargo run -q -p bubble-eval -- editing-ios
```

2026-11-12 本机结果：Rust 核心/协议通过；Web 4/4；Android（Android 16 / SDK 36，Fcitx5 `0.1.3-0-g048f581c`）82 项；iOS 27.2 模拟器 27 项。Android 覆盖 CJK 组合更新、组合内移动/删除/确认、`newCursorPosition`、嵌套 batch、周边文本、代码点删除、`CursorAnchorInfo`、旧连接拒绝，以及通过真实 `AccessibilityNodeProvider` 校验语义节点、方向滚动 action、滚动结果与焦点 action；iOS 覆盖标记文本内部选区与光标/选区几何。视觉选区手柄拖动与系统编辑菜单现已接入；实体设备、厂商键盘与读屏器实测仍属 M2 收尾。

## 6. 更新平面

流程：

1. 下载 manifest 与文件（可后台）。
2. 验 Ed25519 签名（`keyId` 对应公钥）。
3. 校验每个文件 SHA-256 与大小。
4. 检查 `schema`、`minRuntime`、`platforms`、版本单调性。
5. 写入暂存区。
6. 宿主决定 `activate` 或 `defer`；默认下次挂载激活。
7. 原子切换指针；保留 LKG。
8. 新包挂载失败或 N 秒内崩溃 → 自动回 LKG，并上报。

策略：

- 灰度按 app 版本、平台、地域、用户分桶；服务端下发激活策略。
- kill switch：停止下发新版本；已下载未激活的包可作废。
- 离线：使用当前版本或 LKG；不阻塞首屏。
- 磁盘：当前 + LKG + 暂存；其余按 LRU 清理。
- 密钥：内置多把公钥；支持轮换与吊销；manifest 带 `keyId`。

## 7. 可观测性

结构化事件：

- bundle：download、verify、stage、activate、rollback、reject（原因码）。
- 脚本：error、timeout、OOM、restart、事件耗时。
- 桥：method、耗时、错误码、超时、取消。
- 帧：build、layout、paint、a11y 同步、op 队列深度。
- 内容：版本、页面/路由、a11y 节点数。

要求：

- 两个线程都能上报；脚本故障不能阻断上报。
- native FFI 边界捕获 panic 并转为错误事件。
- 不采集用户输入内容与敏感字段；错误栈做脱敏。
- Web 端使用 `sendBeacon`/`fetch keepalive`；native 走宿主上报通道。

## 8. 安全边界

- 只接受签名 bundle；拒绝未签名、哈希不符、版本回放。
- TLS；宿主可选择证书 pinning。
- v1 ambient 能力是明确接受的临时风险；后续加 manifest 允许列表与宿主授权。
- 脚本不能直接调用原生 FFI；只能走能力桥。
- QuickJS 内存/栈/超时限制。Web 主线程脚本没有等效的硬终止。
- FFI 使用安全封装、fuzz、边界隔离；native 不做强沙箱。
- App Store：不使用 JIT；远程 JS 为第一方签名内容；准备 data-only 降级模式并提前做合规评估。
