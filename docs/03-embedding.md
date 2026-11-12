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
- Web：与画布同步的透明 ARIA DOM 覆盖层；Canvas 设 `aria-hidden="true"`。

Web 覆盖层要求：

- 每个语义节点一个绝对定位元素，bounds 与绘制同步。
- 使用 `role`、`aria-label`、`aria-valuenow`、`aria-checked`、`tabindex` 等。
- 焦点环由 runtime 绘制；DOM 只承载语义与焦点。
- `aria-live` 用于 Toast/状态更新；原生端用 announcement API。

焦点与键盘：

- Rust 维护焦点树与 tab 顺序。
- Tab/Shift-Tab、Enter/Space、Esc、方向键在 UI 线程处理。
- 读屏 action（activate、scroll、setValue、increment/decrement）路由到 Rust/脚本。
- 弹层/模态需要焦点陷阱与背景 `aria-hidden`。

## 5. 文本输入与 IME

编辑状态在 **UI 线程**，避免每个按键跨线程：

```text
EditingState {
  value, selection, composition, revision
}
```

平台输入代理：

| 平台 | 代理 | 原生交互 |
|---|---|---|
| iOS | `UIView` 实现 `UITextInput` / `UIKeyInput` | 原生光标、选区手柄、上下文菜单 |
| Android | `InputConnection`（自定义 View 或隐藏 EditText） | 原生手柄与菜单 |
| Web | 隐藏 `textarea`，定位到光标 | 自绘选区与菜单；composition 事件 |

数据流：

1. 用户输入 → 平台代理 → UI 线程编辑状态。
2. UI 线程绘制文本、光标、选区。
3. UI 线程向脚本异步发送 `EditingChanged`。
4. 脚本可下发 `SetEditingValue(handle, epoch, ...)`。
5. epoch 落后则忽略；composition 期间以 UI 线程为准。

覆盖能力：键盘类型、autofill、密码、多行、maxlength、输入过滤、撤销/重做。三端行为差异记录在测试矩阵中。

当前接入：Web 隐藏 `textarea` 把 `beforeinput` / composition 映射到 `edit`（`examples/web/ime.js`）。Android `InputConnection` 提交、composition 与删除。iOS 仅 `UIKeyInput`（插入与退格）；标记文本与选区手柄仍待 `UITextInput`。点按 `focus_at` 聚焦并把插入符落到最近的 UTF-16 边界。聚焦字段在 layout 后绘制选区与 1px 光标，`caret_box` 给 Web 定位 textarea。`field_flags` 的 bit0 多行、bit1 密码，bits 8..15 为键盘种类：1 number、2 email、3 url、4 phone。

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
