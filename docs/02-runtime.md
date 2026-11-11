# 02 · 运行时契约

## 1. 内容管线

### 编译期

输入是 TSX / 组件源码，输出两个产物：

- `template.ir.bin`：静态结构、属性、样式槽、动态绑定槽、事件 handler 表、scope 映射。
- `main.js`：组件初始化与更新函数，目标为 QuickJS 与浏览器 Worker 的公共子集。

采用 Solid 式模型：

- 组件只执行一次，不在每次状态变化时重建虚拟树。
- 状态用信号（signal）表达；编译后的表达式订阅信号，变更时直接产出细粒度 op。
- 没有 VDOM diff。`For` / `Show` 等控制流编译成显式的 keyed 创建、插入、移动、销毁。
- 编译器给静态节点分配模板内 ID；runtime 挂载时返回 `静态 ID → handle` 映射。

### 运行时

1. Rust 按模板 IR 实例化静态节点，返回 handle 映射。
2. 脚本执行组件初始化，把信号绑定到 handle 槽位，注册事件 handler。
3. 信号变化时，编译后的更新函数把变更写入 op 批次。
4. 批次在帧边界合并、压缩，提交给 UI 线程。
5. UI 线程按序应用 op，标记布局/绘制脏区。

布局查询没有同步 API：`measure()` 返回上一已提交帧的结果，或返回 Promise。脚本线程不得阻塞等待 UI 线程。

## 2. Bundle

### 目录结构

```
manifest.json
template.ir.bin
main.js
assets/
  <sha256>...
```

### manifest.json

```json
{
  "schema": 1,
  "id": "com.example.promo",
  "version": 42,
  "minRuntime": "1.0.0",
  "platforms": ["web", "ios", "android"],
  "entry": "main.js",
  "files": [
    { "path": "template.ir.bin", "sha256": "...", "size": 12345 },
    { "path": "main.js", "sha256": "...", "size": 6789 }
  ],
  "keyId": "release-2026",
  "signature": "base64..."
}
```

### 规则

- 签名对象是去掉 `signature` 字段后的 manifest，按 RFC 8785 JCS 规范化，Ed25519 签名。
- 公钥内置在宿主/运行时；`keyId` 支持轮换与吊销。
- 验签通过后再逐文件校验 SHA-256 与大小；任何不一致都拒绝激活。
- 版本单调递增；拒绝回放与降级，除非宿主显式允许。
- 检查 `schema`、`minRuntime`、`platforms`；不兼容直接拒绝并上报。
- 资源按内容寻址；同一 hash 只存一份。
- 激活采用目录指针原子切换；保留当前版本 + LKG + 暂存区。
- 新包挂载失败或启动 N 秒内崩溃，自动切回 LKG。

## 3. Op 协议

### 编码原则

- 二进制、小端、长度前缀；每批次带字符串表，避免重复字符串。
- 批次头：magic `BSOP`、version、seq、opCount、stringTableLen、payloadLen。
- 值使用 tagged union：null / bool / i64 / f64 / string / array / map / handle / color / dimension。
- 不传 JSON；热路径不分配临时对象。
- 每帧一个 arena，双缓冲；生产端按 `(handle, key)` 合并重复 `SetProp`。
- UI 线程按序应用，不丢 op；队列高水位时脚本线程让出执行权（QuickJS interrupt / Worker 消息节流）。

### Handle 分配

- handle 是 u32，0 保留；脚本线程在实例作用域内单调分配。
- Rust 校验唯一性并持有 `handle → node` 映射；不重用已销毁 handle，避免悬空引用。
- 静态挂载时 Rust 返回 `模板 ID → handle`；动态节点由脚本分配后 `CreateNode`。

### Op 列表

| Op | 载荷 | 说明 |
|---|---|---|
| `CreateNode` | handle, templateId, parent, index | 创建节点；templateId 提供默认属性 |
| `DisposeNode` | handle | 销毁子树 |
| `InsertNode` | handle, parent, index | 插入已存在节点 |
| `MoveNode` | handle, parent, index | 移动节点 |
| `SetProp` | handle, key, value | 属性或样式 |
| `SetStyle` | handle, styleId/value | 样式片段 |
| `SetText` | handle, text | 文本内容 |
| `BindEvent` | handle, handlerId | 事件表 |
| `SetSemantics` | handle, role, label, value, state | 无障碍语义 |
| `Animate` | handle, spec | 声明式动画，UI 线程执行 |
| `ScrollTo` | handle, x, y, animated | 滚动容器 |
| `Focus` | handle | 焦点 |
| `SetEditingValue` | handle, epoch, value, selection | 受控文本 |
| `SetAsset` | handle, contentHash | 图片/资源 |

### 事件协议

```text
Event {
  seq: u32,
  handlerId: u32,
  handle: u32,
  type: "tap" | "longpress" | "scroll" | "key" | "focus" | "input" | "a11y",
  payload: tagged value,
  ts: f64
}
```

- UI 线程命中后异步入队，不等待脚本。
- 脚本线程可以丢弃过期事件（例如滚动中间态），但结构性事件不可丢。
- 事件处理期间产生的 op 在帧边界统一提交。

### 文本编辑协议

```text
EditingChanged {
  handle, revision, value, selection, composition
}

SetEditingValue {
  handle, epoch, value, selection
}
```

- 编辑状态由 UI 线程持有；`revision` 单调递增。
- 脚本用 `epoch` 下发受控值；epoch 落后则忽略。
- composition 未提交时以 UI 线程为准；提交后再通知脚本。
- 脚本不能在 composition 期间强制改写文本，除非显式取消输入。

## 4. 线程模型

| 线程 | 职责 | 禁止 |
|---|---|---|
| UI 线程 | 应用 op、布局、绘制、命中、手势、滚动、动画、语义树、编辑状态 | 长任务；同步等待脚本 |
| 脚本线程 | 事件循环、信号、组件、op 生产、宿主调用 | 直接操作 UI 树、画布或原生视图 |

### Native

- 双端统一 QuickJS/PrimJS；每实例一个 context，跑在独立脚本线程。
- C ABI 暴露 Rust core；Swift/Kotlin 壳负责 surface、生命周期、原生输入与 a11y。
- 脚本线程自建事件循环：timers、promises、microtasks、宿主回调。
- `JS_SetInterruptHandler` 做硬超时；`JS_SetMemoryLimit` + 栈上限做资源配额。
- 宿主能力调用异步排队到 UI 线程/宿主线程，结果回调脚本线程。

### Web

- Rust core 编译为 WASM，跑在主线程（UI 线程）。
- 脚本跑在 Web Worker；`postMessage` + transferable `ArrayBuffer` 传 op/事件。
- Worker 内没有 DOM；剪贴板、焦点、ARIA 覆盖层、Canvas2D 都在主线程，由 runtime 代理。
- Worker 超时用 `terminate()` + 重建；不依赖 SharedArrayBuffer（避免 COOP/COEP 部署要求）。

### 启动序列

1. 宿主创建 surface，传入尺寸、insets、scale、能力实现。
2. runtime 加载并验签 bundle，或使用 LKG。
3. 初始化引擎（QuickJS context / Worker + WASM）。
4. 执行 `main.js`，挂载模板，拿到静态 handle 映射。
5. 首批 op 提交；UI 线程布局、绘制首帧。

### 帧循环

runtime 自持 vsync：`CADisplayLink` / `Choreographer` / `requestAnimationFrame`。

每帧：

1. 取出最新 op 批次（合并中间批次，保留结构 op 顺序）。
2. 应用 op，标记布局/绘制脏区。
3. 布局（含平台文本测量）。
4. 绘制脏区。
5. 同步语义树/ARIA 覆盖层。
6. 上报帧统计。

没有 op、没有动画、没有滚动时跳过绘制。

## 5. 布局、绘制与文本

### CSS 子集

| 类别 | 支持 |
|---|---|
| display | flex、none |
| 尺寸 | width/height/min/max、padding、margin |
| flex | direction、wrap、grow/shrink/basis、align/justify、gap |
| 定位 | relative、absolute、z-index |
| 溢出 | visible、hidden、scroll 容器 |
| 视觉 | background、border、radius、opacity、2D transform |
| 文本 | font-family/size/weight/style、line-height、letter-spacing、text-align、color |

不支持：grid、选择器、级联、伪类、隐式继承。样式是显式 props/tokens，不解析 CSS 文本。

### 布局

- Taffy 负责 flex、绝对定位、尺寸约束。
- 文本与图片通过 measure 回调参与布局。
- 脏节点向上/向下传播；布局结果缓存到帧结束。

### 文本

- 测量、整形、换行、bidi、字体回退全部走平台：
  - iOS：CoreText / TextKit。
  - Android：StaticLayout / Paint。
  - Web：Canvas2D `measureText` + 浏览器整形。
- 缓存 key：`(text, style, maxWidth, fontEpoch)`。
- 宿主注入字体时递增 `fontEpoch`，清空文本缓存。
- 三端换行/基线差异是接受的；模板避免依赖固定文本高度。

### 绘制

- 每个实例一个平台画布：iOS `UIView.draw` + CoreGraphics，Android `onDraw(Canvas)`，Web Canvas2D。
- 脏矩形 + 静态子树缓存；全量重绘作为兜底。
- 图片离屏解码，UI 线程只做上传/绘制。
- v1 不做模糊、滤镜、液态玻璃等高级合成。

### 命中与滚动

- 命中按绘制逆序，考虑 transform、clip、z-index。
- 指针捕获、手势识别在 UI 线程。
- 滚动由 UI 线程本地执行，不依赖脚本线程。
- 虚拟化列表协议：UI 线程计算可见区间并发送 `VisibleRangeChanged`；脚本创建/销毁 item 节点；未测量项用估算高度占位。滚动位置归 UI 线程，脚本只能 `ScrollTo`。

## 6. 故障与资源

- 脚本单事件软预算 32ms（仅埋点），硬超时 1s（中断）；初始值，可按实测调整。
- QuickJS 内存上限初始 64MB，栈上限单独设置；超限视为脚本故障。
- 故障处理：上报 → 销毁 context → 重建 → 重新挂载 bundle → 错误占位与重试。应用状态丢失，内容级恢复。
- Web：`Worker.terminate()` 后重建；主线程 UI 不受影响。
- FFI 边界 `catch_unwind`，不允许 Rust panic 跨 C ABI；QuickJS 绑定用安全封装并 fuzz。
- native 为进程内运行，没有强沙箱；依靠签名内容、内存配额、边界隔离降低风险。
- 配额：图片缓存、bundle 磁盘、网络并发；超限淘汰最旧资源。

## 7. 性能假设

- Taffy 布局 1k 节点应在数毫秒级；文本测量是主要成本，必须缓存。
- op 编码与跨线程传输应远小于布局/绘制成本；避免 JSON 与每帧大对象。
- QuickJS 解释执行足够处理事件与状态；重计算应放到宿主或预计算。
- CoreGraphics/Canvas2D 全表面重绘可能成为瓶颈；依赖脏矩形与静态缓存守住 60fps。
