# 02 · 运行时契约

## 1. 内容管线

### 编译期

作者写的是函数组件，不是一份静态 JSX 树。参照 nsh（`docs/dev/native-pipeline.md`）：

```tsx
import { mount, signal, Text, View } from "bubble";

function Counter() {
  const count = signal(0);
  return (
    <View>
      <TapGesture onTap={() => count.update((c) => c + 1)} />
      <Text>Count: {count()}</Text>
    </View>
  );
}

mount(() => <Counter />);

```

- 组件函数只执行一次，不在状态变化时重建树。
- `signal` 的值留在脚本侧（JS 引用），Rust 图只调度。比较用 `===`（`equals: false` 每次写入都通知，`equals(a, b)` 返回真表示相同）。数组和对象按引用持有，不复制进 Rust；原地修改不会通知，要 `set` 一个新值。`computed` 是图上的 memo，一次变化只跑一遍，并且先于 effect。渲染绑定是 render effect，`effect` 是 user effect，排在渲染绑定之后。`cleanup` 挂在当前 owner 上，owner 重跑或销毁时先执行。不引入 `solid-js`。
- 一次 Oxc pass 去掉 TypeScript，把 JSX 降成原生渲染模块的调用：静态属性和子节点设一次；会读信号的表达式登记为绑定。
- 绑定重跑时，原生渲染器直接产出对应 op（`SetText` / `SetProp` / 插入 / 移动 / 销毁）。没有 VDOM diff，也没有「模板 IR + 按绑定生成的 JS 更新函数」。
- `<Show>` / `<For>` / `<Index>` / `<Switch>` / `<Match>` 是运行时控制流：`when` / `each` 是每次重跑都调用的 accessor，不走 `dynamic()`。三元、`&&` 和 `when()` / `each()` 不再是控制流。

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
- 节点只由函数组件首次执行时的 `Spawn` / `SpawnAt` 创建；`Show` / `For` / `Index` / `Switch` 只移动、插入或销毁。函数组件没有复用的模板注册表，也不预编译子树。

### Op 列表

| Op | 载荷 | 说明 |
|---|---|---|
| `Spawn` | `BNOD` 描述符 | 动态节点；`id`/`parent` 由脚本填写，Rust 校验 handle |
| `SpawnAt` | `BNOD` 描述符 + index | 在指定子索引插入；fragment 锚点使用 |
| `DisposeNode` | handle | 销毁子树 |
| `InsertNode` | handle, parent, index | 插入已存在节点 |
| `MoveNode` | handle, parent, index | 移动节点 |
| `SetProp` | handle, key, value | 属性或样式 |
| `SetStyle` | handle, styleId/value | 样式片段 |
| `SetText` | handle, text | 文本内容 |
| `BindEvent` | handle, handlerId | 事件表 |
| `SetSemantics` | handle, role, label, value, state | 无障碍语义 |
| `Animate` | handle, spec | 声明式动画，UI 线程执行 |
| `ScrollTo` | handle, x, y | 滚动容器；按轴 clamp，未启用轴归零 |
| `Focus` | handle | 焦点 |
| `SetEditingValue` | handle, epoch, value, selection | 受控文本 |
| `SetAsset` | handle, contentHash | 预留；当前 `Image` 通过 `SetProp src` 交给宿主解析 |

当前实现覆盖 `Spawn`、`SpawnAt`、`DisposeNode`、`MoveNode`、`SetText`、`SetProp`、`SetPropColor`、`BindEvent`、`ScrollTo`、`Focus`、`SetEditingValue`，以及手势声明 op。`TextField`（kind 4）的编辑状态在 UI 线程，由 `EditSession` 持有 UTF-16 文档位置、方向性选区、独立组合区、`revision` 与撤销历史；`BSED` v2 每批向脚本投递有序 delta 和最终快照。iOS `UITextInput`、Android `InputConnection`、Web 隐藏 textarea 已接入文本编辑。语义树由节点属性派生并经 `bubble_semantics` / wasm `semantics` 输出 JSON；iOS `UIAccessibility`、Android `AccessibilityNodeProvider` 与 Web ARIA 覆盖层读取同一快照。其余 op（含 `InsertNode`、动画、显式 `SetSemantics`）随后续里程碑。

节点描述为 `BNOD` v2：固定字段后跟 `u32` 长度的 TLV 扩展块，新增字段不移动既有偏移。`Spawn` 载荷复用同一 v2 body；`BSOP` 也是 v2。滚动描述符的单个标志字节是位掩码：bit 0 为纵向 `overflow="scroll"`，bit 1 为横向，其余位保留且解码时拒绝。

属性分两类：

- 手势是虚拟子节点：`TapGesture`、`DoubleTapGesture`、`LongPressGesture`、`PanGesture`、`TransformGesture`。没有 `View onPress`。事件名为 `onTap` / `onTapStart` / `onTapEnd` / `onTapCancel`、`onDoubleTap*`、`onLongPress` / `onLongPressEnd` / `onLongPressCancel`、`onPanStart` / `onPanUpdate` / `onPanEnd` / `onPanCancel`、`onTransformStart` / `onTransformUpdate` / `onTransformEnd` / `onTransformCancel`。Transform 用 `mode`（字符串或字符串数组：`pan` / `pinch` / `rotate`）。`direction` 是方向字符串数组。`captureAxis="none"|"x"|"y"|"both"` 把该轴滚动让给这条手势。
- `SPAWN_PROPS`：`overflow`、`overflowX`、`overflowY`、`fontSize`、`radius`、`opacity`、`width`、`height`、`flexGrow`、`flexShrink`、`flexDirection`、`padding`。
- `DEFERRED_PROPS`：`backgroundColor`、`color`、`lineHeight`、`textAlign`、`fontWeight`、`margin`、`gap`、`alignItems`、`justifyContent`、`position`、`inset`、`src`、`fit`、`overscrollX`、`overscrollY`、`multiline`、`secure`、`maxLength`、`keyboard`、`autofill`、`role`、`label`、`disabled`、`hidden`、`focusable`、`modal`、`checked`、`selected`、`expanded`。
- 无障碍属性：`role`（`button`、`link`、`textbox`、`checkbox`、`heading`、`dialog` 等）、`label`、`disabled`、`hidden`（仅语义隐藏）、`focusable`、`modal`（焦点陷阱）、`checked`、`selected`、`expanded`。`Text`、`Image`、`TextField` 分别派生 text/image/textbox 角色；`TextField` 的语义值在 `secure` 时为空。可滚动容器即使没有角色或标签也会进入语义树，并输出对应方向的滚动 action 位。

手势事件以 `BSGE` v1 传输，start/update/end/cancel 按序投递。内部 slot 0 是声明销毁确认：排在该声明的终结事件之后，不调用应用回调；JS 收到确认后才释放 handler，因此在事件回调中销毁声明也能收到 cancel。胜出的手势只产生一次 end 或 cancel，双指中任一配对触点离开后，剩余触点不恢复单指识别。

`color` / `backgroundColor` 在 spawn 描述符里保留默认槽，但作者传入的值一律作为紧随 `Spawn` 的 `SetProp` 下发，由 Rust 统一解析：数字仍按 `0xAARRGGBB`（alpha 在高字节），字符串按 CSS 颜色语法。支持命名色、`transparent`、`#rgb`/`#rgba`/`#rrggbb`/`#rrggbbaa`、`rgb()`/`rgba()`、`hsl()`/`hsla()`、`hwb()`、`lab()`、`lch()`、`oklab()`、`oklch()`，以及 `color()` 的 `srgb`、`srgb-linear`、`display-p3`、`a98-rgb`、`prophoto-rgb`、`rec2020`、`xyz`、`xyz-d50`、`xyz-d65` 空间；逗号与空格/斜杠语法都接受。`currentColor`、相对颜色、`color-mix()`、系统色与 CSS 变量不支持——这些依赖级联或计算样式，请在脚本里用 `computed()` 求值。所有颜色归一为扩展非线性 sRGB 交给宿主：支持广色域的宿主保留越界分量，只画 sRGB 的宿主自行裁剪。

`overflow` 取值 `"visible"` 或 `"scroll"`。`"hidden"` 预定为裁切且不滚动，当前拒绝。`flexDirection` 为 `"row"` 或 `"column"`。`overscrollX` / `overscrollY` 为布尔：`true`（默认）把剩余增量交给外层，`false` 在当前容器截断。`padding` / `margin` / `inset` 按 CSS 简写（1、2 或 4 个数，4 个数为上右下左）。

基础原语为 `View`、`Text`、`Image`。`Image` 的 `src` 是宿主可解析的路径/URL，`fit` 为 `contain`、`cover` 或 `fill`；解析失败时该帧跳过图片，不阻断其他绘制。

`Show` / `For` / `Index` / `Switch` 是透明 fragment。`Show` 在条件保持为真时不重建，并把真值 accessor 交给子函数；`For` 的 `by` 必填；`Index` 按下标复用；`Switch` 取第一个为真的 `Match`。

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
- 当前实现只有 `tap`：handler 收到 `{ type: "tap", x, y }`。`BindEvent` 绑定单个 handler；节点销毁时 runtime 释放 handler 表项。
- 脚本线程可以丢弃过期事件（例如滚动中间态），但结构性事件不可丢。
- 事件处理期间产生的 op 在帧边界统一提交。

### 文本编辑协议

`BSED` v2 每批携带一条或多条编辑事件。每条事件是增量文本操作加最终快照：

```text
BSED { version: 2, count }
EditingChanged {
  handle, handler, session, revision,
  deltas: [{ start, end, insert }],
  selection: { anchor, active },
  composition: { start, end } | null,
  composing,
  value
}
SetEditingValue {
  handle, epoch, value, selection
}
```

- `deltas` 按旧 UTF-16 文档坐标顺序应用；`value` 是该批结束后的最终值，只发送一次，供受控字段和诊断使用。
- `selection` 保留方向（`anchor` 固定端、`active` 光标端）；`composition` 与选区独立，`composing` 在组合尚未产生范围时也为真。
- 编辑状态由 UI 线程持有；`revision` 单调递增。一个批次只提交一次 revision、一条事件和一步撤销。
- 批次 API：C ABI `bubble_edit_batch_begin` / `bubble_edit_batch_end`，WASM `edit_batch_begin` / `edit_batch_end`，JNI `editBatchBegin` / `editBatchEnd`。嵌套批次只在最外层发布事件。
- 批次内读取反映当前事务状态；命令失败、连接关闭或失焦会丢弃整个未提交批次。
- 脚本用 `epoch` 下发受控值；epoch 不匹配返回 stale conflict。composition 未提交时，普通脚本写入返回 composition conflict；显式 `cancel` 先取消组合再 reset。
- 被拒绝的脚本写入进入 `Shell::take_edit_conflicts`，不静默丢弃，也不记录用户文本。
- 版本 2 与生产端、Web 解码端锁步切换；旧 BSED v1 路径已删除。

C ABI/JNI/WASM 的编辑事务入口按活动会话校验身份；iOS 与 Web 已迁移到相同的事务、组合区与 revision 规则。阶段 E 已把三端编辑几何统一到平台 `LayoutSnapshot`：同一平台结果驱动绘制、命中和 IME 坐标。

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
2. 应用 op，失效 paint cache。
3. 布局（含平台文本测量）。
4. 绘制：当前整表面重绘；矩形脏区未实现。
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
| 溢出 | `overflow` / `overflowX` / `overflowY` 为 `visible` 或 `scroll`。`hidden` 预定为裁切且不滚动，当前拒绝 |
| 视觉 | `backgroundColor`、border、radius、opacity、2D transform |
| 文本 | font-family/size/weight/style、line-height、letter-spacing、text-align、color |

不支持：grid、选择器、级联、伪类、隐式继承、CSS 变量。样式是显式 props/tokens，不解析 CSS 样式表；只有颜色值的字面量按 CSS 颜色语法解析。当前已实现 flex 方向、grow/shrink、尺寸、padding、margin、gap、align/justify、relative/absolute 定位与 inset。border、transform、min/max 仍在路线图。`overflow="hidden"` 的预定行为是裁切且不滚动，当前拒绝。

### 布局

- Taffy 负责 flex、绝对定位、尺寸约束。
- 文本通过 measure 回调参与布局；回调携带 `font_size`、`line_height`、`font_weight` 与可用宽度。图片由显式 `width`/`height`（或父级约束）决定盒子，不做内禀尺寸测量。
- 脏节点向上/向下传播；布局结果缓存到帧结束。
- `Shell::paint` 缓存已生成的 `PaintCmd` 列表；op、viewport、layout、滚动变化会使缓存失效。当前只避免重复遍历树，宿主仍清整表面；矩形脏区与静态子树缓存待在绘制层实现。

### 文本

- 测量、整形、换行、bidi、字体回退全部走平台：
  - iOS：CoreText / TextKit。
  - Android：StaticLayout / Paint。
  - Web：Canvas2D `measureText` + 浏览器整形。
- `PaintCmd::Text` 携带节点盒、行高、字重与对齐；三端用同一组参数换行并绘制。
- 缓存 key：`(text, style, maxWidth, fontEpoch)`。
- 宿主注入字体时递增 `fontEpoch`，清空文本缓存。
- 三端换行/基线差异是接受的；内容避免依赖固定文本高度。

### 绘制

- 每个实例一个平台画布：iOS `UIView.draw` + CoreGraphics，Android `onDraw(Canvas)`，Web Canvas2D。
- 脏矩形 + 静态子树缓存；全量重绘作为兜底。
- `PaintCmd` 当前包含填充、文本、图片、裁剪与透明度；图片按宿主可解析的 `src` 解码，离屏缓存。
- v1 不做模糊、滤镜、液态玻璃等高级合成。

### 命中与滚动

- 命中按绘制逆序，考虑 transform、clip、z-index。
- 指针捕获、手势识别在 UI 线程。
- 滚动由 UI 线程本地执行，不依赖脚本线程。横纵两轴各自维护偏移与范围；`overflowX` / `overflowY` 为 `"scroll"` 时启用，`"visible"` 的轴不产生滚动且 `ScrollTo` 将其归零。
- 滚轮/拖拽按实际命中路径逐轴消费 `(dx, dy)`：内层先 clamp，未消费部分在同次输入继续交给祖先；`overscrollX/Y={false}` 在对应轴截断剩余量。触摸识别时冻结该轴策略，wheel 每次读取最新已提交配置。
- `Shell::scroll_of` 返回 `(left, top)`；宿主分别通过 `bubble_scroll_x_of` / `bubble_scroll_y_of` 读取。
- 读屏滚动 action 复用同一 `scroll_path`：目标为语义节点最近的、接受该轴的可滚动祖先，步长为可见范围的 80%；无可用祖先或轴时返回错误，平台不得自行累加像素。
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
