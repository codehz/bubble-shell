# 02 · 运行时契约

## 1. 内容管线

### 编译期

作者写的是函数组件，不是一份静态 JSX 树。参照 nsh（`docs/dev/native-pipeline.md`）：

```tsx
import { mount, signal, Text, View } from "bubble";

function Counter() {
  const count = signal(0);
  return (
    <View onPress={() => count.update((c) => c + 1)}>
      <Text>Count: {count()}</Text>
    </View>
  );
}

mount(() => <Counter />);

```

- 组件函数只执行一次，不在状态变化时重建树。
- `signal` / `computed` / `effect` 是 Rust 里的图，JS 只是调用入口。不引入 `solid-js`，也不在 JS 里维护订阅。
- 一次 Oxc pass 去掉 TypeScript，把 JSX 降成原生渲染模块的调用：静态属性和子节点设一次；会读信号的表达式登记为绑定。
- 绑定重跑时，原生渲染器直接产出对应 op（`SetText` / `SetProp` / 插入 / 移动 / 销毁）。没有 VDOM diff，也没有「模板 IR + 按绑定生成的 JS 更新函数」。
- `<If>` / `<Each>` 是运行时原语：按真值和 key 创建、移动、销毁，不在编译期把列表展开成静态节点。

### 运行时

1. 脚本线程执行应用模块。`mount` 调用组件一次，渲染器按调用创建节点并登记绑定，产出首批 op。
2. 事件到达已登记的 handler；handler 写信号。
3. 图只重跑读过该信号的绑定，把变更写入 op 批次。
4. 批次在帧边界合并、压缩，提交给 UI 线程。
5. UI 线程按序应用 op，标记布局/绘制脏区。

布局查询没有同步 API：`measure()` 返回上一已提交帧的结果，或返回 Promise。脚本线程不得阻塞等待 UI 线程。

## 2. Bundle

### 目录结构

```
manifest.json
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
- native：UI 线程按序应用，不丢 op；队列高水位时 QuickJS interrupt 让出。Web 同线程，无队列。

### Handle 分配

- handle 是 u32，0 保留；脚本线程在实例作用域内单调分配。
- 动态节点由脚本分配 handle 后发送 `Spawn`，载荷带完整 `BNOD` 描述符；Rust 校验后插入树。
- 模板挂载仍返回 `模板 ID → handle`；`CreateNode` 用于从模板派生节点，当前主要保留给测试与后续组件复用。

### Op 列表

| Op | 载荷 | 说明 |
|---|---|---|
| `Spawn` | `BNOD` 描述符 | 动态节点；`id`/`parent` 由脚本填写，Rust 校验 handle |
| `SpawnAt` | `BNOD` 描述符 + index | 在指定子索引插入；fragment 锚点使用 |
| `CreateNode` | handle, templateId, parent, index | 从模板创建节点；templateId 提供默认属性 |
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

当前 M1 实现覆盖 `Spawn`、`SpawnAt`、`CreateNode`、`DisposeNode`、`MoveNode`、`SetText`、`SetPropColor`、`BindEvent`、`ScrollTo`；其余 op 随组件/输入/更新里程碑补齐。脚本线程只产 op，UI/帧线程在 drain 时应用。

`when` / `each` 采用透明 fragment：在父节点下创建一个零尺寸、无背景、无 handler 的 anchor，分支/列表节点通过 `SpawnAt` 插到 anchor 之前；重跑时先 `DisposeNode`，再用 `MoveNode` 复用 keyed item。组件树不额外增加布局包装节点。

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

- 双端统一 QuickJS/PrimJS；v1 单实例嵌入，进程内一个 context，跑在独立脚本线程。多实例仍是待定项。
- C ABI 暴露 Rust core；Swift/Kotlin 壳负责 surface、生命周期、原生输入与 a11y。
- 脚本线程自建事件循环：timers、promises、microtasks、宿主回调。
- `JS_SetInterruptHandler` 做硬超时；`JS_SetMemoryLimit` + 栈上限做资源配额。
- 宿主能力调用异步排队到 UI 线程/宿主线程，结果回调脚本线程。

### Web

- 没有 Worker，没有第二份 WASM。主线程：浏览器 JS + 一份 WASM（信号图、保留树、Taffy、命中、滚动）。
- 信号更新直接改树上的节点，同一轮接着布局。绘制仍把 `PaintCmd` 交给 JS 回放。文本测量用页面 Canvas 的 `measureText`。
- 主线程 JS 同时回放绘制、同步 ARIA、接指针/滚轮/IME。
- 脚本长任务卡住绘制和滚动，直到这次调用返回。不做切片，不把滚动留在另一线程。

### 启动序列

1. 宿主创建 surface，传入尺寸、insets、scale、能力实现。
2. runtime 加载并验签 bundle，或使用 LKG。
3. 初始化引擎（native：QuickJS context；Web：主线程 WASM）。
4. 执行 `main.js`：`mount` 跑根组件一次，渲染器建树并登记绑定。
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
- QuickJS 内存上限初始 64MB，栈上限单独设置；内存超限与硬超时走同一故障路径：中断执行、销毁 context、重建并重新挂载 bundle。
- 故障处理：上报 → 销毁 context → 重建 → 重新挂载 bundle → 错误占位与重试。应用状态丢失，内容级恢复。
- Web：一次调用返回前页面卡住；无法像 Worker 那样 `terminate()`。硬超时只存在于 native QuickJS。
- FFI 边界 `catch_unwind`，不允许 Rust panic 跨 C ABI；QuickJS 绑定用安全封装并 fuzz。
- native 为进程内运行，没有强沙箱；依靠签名内容、内存配额、边界隔离降低风险。
- 配额：图片缓存、bundle 磁盘、网络并发；超限淘汰最旧资源。

## 7. 性能假设

- Taffy 布局 1k 节点应在数毫秒级；文本测量是主要成本，必须缓存。
- op 编码与跨线程传输应远小于布局/绘制成本；避免 JSON 与每帧大对象。
- QuickJS 解释执行足够处理事件与状态；重计算应放到宿主或预计算。
- CoreGraphics/Canvas2D 全表面重绘可能成为瓶颈；依赖脏矩形与静态缓存守住 60fps。
