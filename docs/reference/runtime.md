# 02 · 运行时契约

## 1. 内容管线

### 编译期

作者写的是函数组件，不是一份静态 JSX 树。参照 nsh（`docs/dev/native-pipeline.md`）：

```tsx
import { mount, signal, Text, View } from "bubble-shell";

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
- 作者写多模块 TSX/TS/JS。每个模块一次 Oxc pass 去掉 TypeScript，把 JSX 降成原生渲染模块的调用；任意其他语句、表达式和 import 原样保留。
- rolldown 把模块图链接成单个 ES module；`bubble-shell` / `bubble:rt` 保持导入，由宿主提供，运行时本身不进 bundle。发布态不做代码分割、minify 或 QuickJS 字节码；开发态 Web 使用多模块 HMR，见 [07-js-bundling-plan.md](07-js-bundling-plan.md)。
- 静态属性和子节点设一次；会读本地 `signal()` / `computed()` 的表达式登记为绑定。
- 绑定重跑时，原生渲染器直接产出对应 op（`SetText` / `SetProp` / 插入 / 移动 / 销毁）。没有 VDOM diff，也没有「模板 IR + 按绑定生成的 JS 更新函数」。
- `<Show>` / `<For>` / `<Index>` / `<Switch>` / `<Match>` 是运行时控制流：`when` / `each` 是每次重跑都调用的 accessor，不走 `dynamic()`。三元、`&&` 和 `when()` / `each()` 不再是控制流。
- 组件是普通函数：大写标签编译成 `component(Fn, props, children?)`，在父节点构建时同步执行一次。组件 props 采用统一 getter：非函数表达式一律编译成 `() => expr`，组件用 `props.x()` 读取（JSX 值例外，编译成渲染 thunk；`children` prop 维持 JSX / 函数值 / `props.children` 转发三选一）。裸 signal 绑定标识符作 prop（`live={live}`）编译报错，须写 `live={live()}`。读 signal 的原语 prop 仍编译成 `dynamic(() => expr)`，由 runtime 内部的 `bindProp` / `bindGestureProp` 消费；`dynamic` / `Dynamic` 不属于作者 API（详见 [plans/jsx-transform.md](plans/jsx-transform.md)）。
- 子节点位置写 `{props.children}` 或 `{props.children()}` 即在该处渲染 children；`children` / `*.children` 是保留的槽引用，不能当文本或原语属性用。命名槽用 `<Slot of={props.header} />` 或 `<Slot>{props.header}</Slot>`，可选 `fallback={<Text>…</Text>}` 在槽为空时渲染。槽没有自己的节点；槽内容放进 `<Show>` / `<For>` 时跟随分支的插入与销毁。控制流 render 里写 `{() => props.children}` 会自动降成槽调用；其余裸函数返回值（如 `{() => props.header}`）不会渲染，要写 `{() => props.header()}` 或 `<Slot of={props.header} />`。
- children 中的 `{expr}` 按编译期分类：元素绑定（`const pad = <Pad/>`，声明处降成 thunk，使用点 `content(pad)`）、显式 thunk 或含 JSX 的表达式（`{cond ? <A/> : <B/>}`）→ `content(thunk)` 分支，依赖变化时销毁重建；读 signal 的值表达式与字符串 → 就地响应式 `<Text>`；无法静态分类的函数调用（`{make()}`）与被当作文本的元素绑定（`<Text>{pad}</Text>`）编译报错。`content()` 每次重建整段节点，高频更新应走 `<Text>` 字符串路径。

### 运行时

1. 脚本线程执行入口 bundle。`mount` 调用组件一次，渲染器按调用创建节点并登记绑定，产出首批 op。
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
  logo.1a2b3c4d.png
```

发布态 `main.js` 是单个入口 ES module；fixture 构建沿用 `<name>.js`，manifest 的 `entry` 指向实际文件名。开发态由 `bubble dev` 提供多模块 factory、`bubble:hot` 运行时与热补丁；开发态 source map 尚未接入。热更新会原位重跑变更模块的 factory，模块导出需按字段赋值（`module.exports.foo = …`）；整体替换 `module.exports` 不会传播到已持有旧引用的模块，此时由错误 overlay / 整页 reload 兜底。运行时模块不在此目录中。

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
  "assets": [
    { "path": "assets/logo.1a2b3c4d.png", "sha256": "...", "size": 1024 }
  ],
  "keyId": "release-2026",
  "signature": "base64..."
}
```

### 规则

- 签名对象是去掉 `signature` 字段后的 manifest，按 RFC 8785 JCS 规范化，Ed25519 签名。`files` 与 `assets` 都在签名范围内。
- 公钥内置在宿主/运行时；`keyId` 支持轮换与吊销。
- `files` 只列出运行时实际加载并在激活时校验的文件；当前就是入口 bundle。`bubble_load` 只接收 manifest 与入口字节，不因资源增加而改变 ABI。
- `assets` 是宿主按需解析的资源，路径相对 bundle 根，命名为 `<name>.<hash8>.<ext>`；同一内容只存一份。宿主读取时可用条目里的 SHA-256 与大小校验。
- 验签通过后逐 `files` 校验 SHA-256 与大小；任何不一致都拒绝激活。
- 版本单调递增；拒绝回放与降级，除非宿主显式允许。
- 检查 `schema`、`minRuntime`、`platforms`；不兼容直接拒绝并上报。
- bundle 是单个 ES module；`bubble-shell` / `bubble:rt` 保持裸导入，由宿主提供。native 用 QuickJS loader 解析，Web 用 import map 解析。发布态不含 source map。
- 激活采用目录指针原子切换；保留当前版本 + LKG + 暂存区。
- 新包挂载失败或启动 N 秒内崩溃，自动切回 LKG。

## 3. Op 协议

### 编码原则

- 二进制、小端、长度前缀；每批次带字符串表，避免重复字符串。
- 批次头：magic `BSOP`、version（当前 4）、seq、opCount、stringTableLen、payloadLen。
- 值使用 tagged union：null / bool / i64 / f64 / string / array / map / handle / color / dimension。
- 不传 JSON；热路径不分配临时对象。
- 每帧一个 arena，双缓冲；生产端按 `(handle, key)` 合并重复 `SetProp`。
- native：UI 线程按序应用，不丢 op；队列高水位时 QuickJS interrupt 让出。Web 同线程，无队列。

### Handle 分配

- handle 是 u32，0 保留；脚本线程在实例作用域内单调分配。
- 动态节点由脚本请求创建，`Spawn` / `SpawnAt` 只携带 `handle`、`parent`、`kind` 和初始文本；Rust 校验后插入树，随后按序应用 `SetProp`。
- 节点只由函数组件首次执行时的 `Spawn` / `SpawnAt` 创建；`Show` / `For` / `Index` / `Switch` 只移动、插入或销毁。函数组件没有复用的模板注册表，也不预编译子树。

### Op 列表

| Op | 载荷 | 说明 |
|---|---|---|
| `Spawn` | handle, parent, kind, text | 动态节点；handle 由 Rust 分配，初始文本随创建下发 |
| `SpawnAt` | handle, parent, kind, text, index | 在指定子索引插入；fragment 锚点使用 |
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
| `CreateVar` | var, source, initial | UI 线程共享变量；`source` 0 为脚本驱动，其他为状态源 |
| `BindVar` | var, target | 关联状态源目标节点；0 解绑 |
| `DisposeVar` | var | 变量退役，依赖它的声明转为不匹配 |
| `SetVar` | var, value, anim | 脚本驱动的变量写入，帧边界生效；写状态源变量报错。`anim` 0 立即、2 tween（时长 f32 + easing u8）、3 spring（刚度 f32 + 阻尼 f32） |
| `SettleVar` | var | 请求收敛通知；无动画时随批次立即发 `BSVE` 事件 |
| `DefineUiFunction` | id, params, program | 注册 UI 程序（u32 长度前缀的捕获/常量/字节码载荷）；校验失败则该批次失败 |
| `BindGestureProgram` | gesture, slot, function | 把手势事件槽绑定到 UI 程序；与脚本 handler 互斥，0 清除 |
| `RunUiFunction` | function, args | JS 线程请求在 UI 线程执行一次 worklet |
| `CancelVarAnim` | var | 停止变量的在途动画，显示值保持不变 |
| `SpawnStyleDecl` | decl, target, component, index | 虚拟样式声明（1 ViewStyle、2 TextStyle）；`index` 是该目标内的声明序号 |
| `SetStyleDeclProp` | decl, key, value | `when` / patch 项；值为静态值或表达式程序 |
| `MoveStyleDecl` | decl, target, index | `index` 是移除自身后的最终位置 |
| `DisposeStyleDecl` | decl | 目标销毁时声明随之销毁 |
| `SetAsset` | handle, contentHash | 预留；当前 `Image` 通过 `SetProp src` 交给宿主解析 |

当前实现覆盖 `Spawn`、`SpawnAt`、`DisposeNode`、`MoveNode`、`SetText`、`SetProp`、`SetPropColor`、`BindEvent`、`ScrollTo`、`Focus`、`SetEditingValue`，以及手势声明 op、共享变量 op、样式声明 op 和 worklet op。`TextField`（kind 4）的编辑状态在 UI 线程，由 `EditSession` 持有 UTF-16 文档位置、方向性选区、独立组合区、`revision` 与撤销历史；`BSED` v2 每批向脚本投递有序 delta 和最终快照。iOS `UITextInput`、Android `InputConnection`、Web 隐藏 textarea 已接入文本编辑。语义树由节点属性派生并经 `bubble_semantics` / wasm `semantics` 输出 JSON；iOS `UIAccessibility`、Android `AccessibilityNodeProvider` 与 Web ARIA 覆盖层读取同一快照。其余 op（含 `InsertNode`、动画、显式 `SetSemantics`）随后续里程碑。

节点创建不再携带描述符：`Spawn` / `SpawnAt` 只有身份和初始文本，其余属性一律是 `SetProp`。`NodeDesc` 保留为 Rust 侧的直接调用构造器，`Turn::spawn` 会把它降级为 `Spawn` 加属性 op；wire 上不存在按类型布局的定长体。滚动、布局和绘制属性因此只有一条解析路径。

属性与虚拟子节点：

- 手势是虚拟子节点：`TapGesture`、`DoubleTapGesture`、`LongPressGesture`、`PanGesture`、`TransformGesture`。没有 `View onPress`。事件名为 `onTap` / `onTapStart` / `onTapEnd` / `onTapCancel`、`onDoubleTap*`、`onLongPress` / `onLongPressEnd` / `onLongPressCancel`、`onPanStart` / `onPanUpdate` / `onPanEnd` / `onPanCancel`、`onTransformStart` / `onTransformUpdate` / `onTransformEnd` / `onTransformCancel`。Transform 用 `mode`（字符串或字符串数组：`pan` / `pinch` / `rotate`）。`direction` 是方向字符串数组。`captureAxis="none"|"x"|"y"|"both"` 把该轴滚动让给这条手势。每个手势原语接受 `ref`：声明创建后 `ref.current` 是该手势声明 id（与节点 handle 是独立 id 空间），`createSharedValue({ source: "gesture.*", target: ref })` 在声明创建时完成绑定，`ref` 尚未解析时先挂起。
- 样式声明与共享变量是虚拟子节点。`createSharedValue({ source, target })` 绑定状态源（`pressed` 是 0..1 进度，`focused` / `disabled` / `editing` 是布尔，`gesture.translationX` / `gesture.translationY` / `gesture.scale` / `gesture.rotation` 读目标手势声明的实时值——pan 取已接受候选的平移、pinch 取 transform state，空闲时 scale 为 1、其余为 0，`scroll.x` / `scroll.y` 读目标滚动容器的偏移），`createSharedValue(0)` 建脚本驱动变量，由 `set` / `bind`（signal 重跑编码 `SetVar`）或 `.value =` 写入，`settle()` 返回收敛 Promise（`BSVE` 事件）。JS 线程可以 `sv.value = withTiming(target, { duration, easing: "linear" | "easeInOut" })` 或 `withSpring(target, { stiffness, damping })` 开始动画、`cancelAnimation(sv)` 停止；UI 线程动画在 worklet 里写同样的赋值（编译期展开为 `storeAnim`，spec 默认 tween 300ms / spring 170·26）。声明组件 `<ViewStyle target={view} when={!disabled && press > 0} opacity={1 - press * 0.4} />` 与 `<TextStyle target={label} when={focused} color={accent} />`：`when` 与 patch 值都是表达式（常量折叠为静态值），标识符解析为作用域内变量或常量；可 patch 的只有 `ViewStyle` 的 `backgroundColor`/`radius`/`opacity`/`fit`/`translateX`/`translateY`/`scale`/`rotate` 与 `TextStyle` 的 `color`/`textAlign`，`textAlign`/`fit` 用 `select(cond, a, b)` 在枚举常量间选择。transform 分量在绘制侧以 `PushTransform`/`PopTransform`（tag 9/10）绕节点中心应用，作用于该节点及其子树，单位变换零成本，不进 Taffy。声明不创建盒子、不命中、不进入语义树；`target` 显式必填，同目标多声明按源码顺序覆盖，后者胜出。匹配中的含 `Focused` 源声明接管默认焦点框（空 patch 也算）；`pressed` 反馈由 UI 线程在首指按下、移出 `TAP_SLOP`、抬起、宿主取消、第二指按下或手势胜出时更新。脚本写状态源变量、写未知变量均报错。
- UI worklet：函数体首条指令是 `"worklet"` 的箭头函数或函数表达式在编译期替换为 `uiFunction(params, captures, body, site)` 调用——`site` 是编译期稳定的定义位置；运行时按 site 缓存程序，重渲染只在捕获或字节码变化时以同一 id 重定义，程序表因此按源码位置而非渲染次数增长。闭包里的自由标识符与字符串字面量成为按值捕获（共享值对象→tag 4、worklet→tag 5、JS 函数→tag 6、数字/布尔/字符串→原始 tag）。worklet 只支持算术/比较/逻辑/`select`、`if/else`、局部 `let/const`（≤16 个）、对捕获的 `.value` 赋值、`param.field` 事件读取、`Math.min/max/abs/floor/ceil/round/sqrt/pow`、`sv.value = withTiming(...)` / `withSpring(...)`、`cancelAnimation(sv)`、`scheduleOnJS(fn, ...args)`、以及调用同模块的其他 worklet（禁止递归，编译期报错）。JS 线程用 `scheduleOnUI(fn, ...args)`（编译为 `RunUiFunction` op）把 worklet 派发到 UI 线程；`BSUE` 批把 `scheduleOnJS` 调用回送 JS（捕获的 JS 回调按引用计数随程序重定义释放）。手势事件槽是 worklet 或 JS 处理器二选一：worklet 处理器在 UI 线程每帧运行，无 JS 参与。
- 属性只有一个入口：`SetProp`。`Prop::parse` 是名称的唯一权威；静态值、动态绑定、简写展开和直接 Rust 调用都走同一条解析路径。`padding` / `margin` / `inset` 数组由 JS 展开为边属性，`gap` 先于 `rowGap` / `columnGap`，`overflow` 先于两轴。
- 无障碍属性：`role`（`button`、`link`、`textbox`、`checkbox`、`heading`、`dialog` 等）、`label`、`disabled`、`hidden`（仅语义隐藏，不裁切，也不改变布局、绘制、命中或焦点）、`focusable`、`modal`（焦点陷阱）、`checked`、`selected`、`expanded`。`Text`、`Image`、`TextField` 分别派生 text/image/textbox 角色；`TextField` 的语义值在 `secure` 时为空。可滚动容器即使没有角色或标签也会进入语义树，并输出对应方向的滚动 action 位。绘制上的隐藏用 `visibility`、`display` 或 `overflow`。

手势事件以 `BSGE` v1 传输，start/update/end/cancel 按序投递。内部 slot 0 是声明销毁确认：排在该声明的终结事件之后，不调用应用回调；JS 收到确认后才释放 handler，因此在事件回调中销毁声明也能收到 cancel。胜出的手势只产生一次 end 或 cancel，双指中任一配对触点离开后，剩余触点不恢复单指识别。

`color` / `backgroundColor` 一律作为 `SetProp` 下发，由 Rust 统一解析：数字按 `0xAARRGGBB`（alpha 在高字节），字符串按 CSS 颜色语法。支持命名色、`transparent`、`#rgb`/`#rgba`/`#rrggbb`/`#rrggbbaa`、`rgb()`/`rgba()`、`hsl()`/`hsla()`、`hwb()`、`lab()`、`lch()`、`oklab()`、`oklch()`，以及 `color()` 的 `srgb`、`srgb-linear`、`display-p3`、`a98-rgb`、`prophoto-rgb`、`rec2020`、`xyz`、`xyz-d50`、`xyz-d65` 空间；逗号与空格/斜杠语法都接受。`currentColor`、相对颜色、`color-mix()`、系统色与 CSS 变量不支持——这些依赖级联或计算样式，请在脚本里用 `computed()` 求值。所有颜色归一为扩展非线性 sRGB 交给宿主：支持广色域的宿主保留越界分量，只画 sRGB 的宿主自行裁剪。

`overflow` / `overflowX` / `overflowY` 各自取值 `"visible"`、`"hidden"` 或 `"scroll"`，两轴独立。`"scroll"` 在该轴裁切并可滚动；`"hidden"` 在该轴裁切且不可滚动；`"visible"` 不裁切该轴，内容在视口内保持无界。裁切矩形是 padding box（当前没有 border，即节点外框），复用已有 `PushClip` / `PopClip`。`fadeEdges` 是逻辑像素，默认 0。仅当对应轴为 `scroll` 且内容确实溢出时，背景和裁剪之后用 `PushLayer` / `PopLayer`（tag 12/13）把内容包进独立层，再在 `PopClip` 之前发出 `FadeEdges`（tag 11）：四边长度是 `min(fadeEdges, 该侧剩余可滚距离)`，贴边时该侧为 0。宿主在该层内用 destination-out 渐变淡出内容；容器背景留在层外，避免擦穿到页面底色。`hidden` 与 `visible` 不虚化。作者对象里的简写 `overflow` 先作用于两轴，再由已声明的 `overflowX` / `overflowY` 覆盖对应轴，与对象键顺序无关。简写是动态绑定时，已声明的轴并入同一条绑定：简写信号变化会先重算简写，再重算这些轴，轴覆盖保持优先；未声明的轴不会被恢复。简写是静态值或省略时，动态轴仍是各自的绑定，只更新该轴。节点创建之后的显式 `setProp` 不加入该绑定，保持调用顺序；下一次作者绑定重跑会按简写然后已声明的轴再次写回。`display` 为 `"flex"`（默认）或 `"none"`；`"none"` 将整棵子树排除出布局、绘制、命中、焦点和语义 action。`visibility` 为 `"visible"`（默认）或 `"hidden"`；`"hidden"` 不绘制、不命中、不聚焦、不发布语义 action，但保留布局占位，后代不能覆盖为可见。动态改为 `display="none"` 或 `visibility="hidden"` 时，清除该子树内的焦点，并取消其中活动的和待决的手势。`flexDirection` 为 `"column"`（默认）、`"row"`、`"column-reverse"` 或 `"row-reverse"`。`flexShrink` 默认 0。`flexBasis` 默认 auto，`alignSelf` 默认 auto，`flexWrap` 默认 `"nowrap"`，`alignContent` 默认 `"flex-start"`。长度是数字（pt）或 `"50%"` 字符串；宽高、min/max、flexBasis 的百分比相对父级对应轴，padding、margin、inset 的百分比相对父级宽度。`gap` 的百分比按该节点自身对应轴的内容区解析。`gap` 先写两轴，`rowGap` / `columnGap` 再覆盖，规则与 overflow 简写相同。`overscrollX` / `overscrollY` 为布尔：`true`（默认）把剩余增量交给外层，`false` 在当前容器截断。`padding` / `margin` / `inset` 按 1、2 或 4 个数，4 个数为上、end、下、start。单边是 `start` / `end`，没有 RTL 时 start 为左。两边尺寸都已确定时忽略 `aspectRatio`。显式 min 覆盖 scroll 和 `flexGrow` 使用的 min 0。

基础原语为 `View`、`Text`、`Image`、`ScrollBar`。`Image` 的 `src` 是宿主可解析的路径/URL，`fit` 为 `contain`、`cover` 或 `fill`；解析失败时该帧跳过图片，不阻断其他绘制。`ScrollBar`（kind 5）必须是滚动容器的直接子节点，自身不占绘制；父节点在内容淡出之后、裁剪弹出之前按轴画滑块。仅当该轴为 `scroll` 且内容溢出时出现。`axis` 为 `vertical`（默认）或 `horizontal`。`scrollbarWidth` 默认 4，`scrollbarMargin` 默认 2，`scrollbarColor` 默认白且 alpha 0.5，`scrollbarRadius` 默认 -1（等于宽度的一半），`scrollbarOpacity` 默认 0.5，`scrollbarMinLength` 默认 20。`autoHide` 默认 true：以壳的 `now_ms` 计，距该容器最后一次滚动不足 1000ms 时显示。`interactive` 默认 true 时，按在滑块上拖动，指针位移按 `(轨道长度 - 滑块长度)` 线性映射到滚动偏移。滑块不进入无障碍树，除非另外设置了 `role` 或标签。

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

- 每个 shell 一个 QuickJS context，跑在自己的脚本线程。C ABI 把 `BubbleShell *` 显式传入每次调用，没有进程级当前实例。
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
4. 执行入口 bundle（`main.js` 或 fixture 的 `<name>.js`）：`mount` 跑根组件一次，渲染器建树并登记绑定。
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
| display | `flex`（默认）、`none`。`none` 将整棵子树排除出布局、绘制、命中、焦点和语义 action |
| visibility | `visible`（默认）、`hidden`。`hidden` 隐藏整棵子树但保留布局；后代不能重新显示 |
| 尺寸 | width/height/min/max、padding、margin |
| flex | direction、wrap、grow/shrink/basis、align/justify、gap |
| 定位 | relative、absolute、z-index |
| 溢出 | `overflow` / `overflowX` / `overflowY` 独立取 `visible`、`hidden` 或 `scroll`。`hidden` 裁切且不滚动；`scroll` 裁切并可滚动；`visible` 该轴不裁切。裁切在 padding box（无 border 时即节点外框）。`fadeEdges` 只虚化仍可滚动的边 |
| 视觉 | `backgroundColor`、border、radius、opacity、2D transform |
| 文本 | font-family/size/weight/style、line-height、letter-spacing、text-align、color、`textOverflow`、`lineClamp` |

不支持：grid、选择器、级联、伪类、隐式继承、CSS 变量。样式是显式 props/tokens，不解析 CSS 样式表；只有颜色值的字面量按 CSS 颜色语法解析。当前已实现 flex 方向、grow/shrink、尺寸、padding、margin、gap、align/justify、relative/absolute 定位与 inset、`display`、`visibility`，按轴独立的 `overflow`（`visible` / `hidden` / `scroll`，裁切在 padding box），以及 `fadeEdges`。border、transform、min/max 仍在路线图。

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
- `textOverflow: "ellipsis"` 只作用于 `Text`。布局之后，用同一次测量把绘制字符串截成容器宽度并追加 `…`。`lineClamp` 为 0 或 1 时整段按单行截断，布局高度也是一行；大于等于 2 时保留前 N−1 行换行结果，只截断第 N 行，布局高度为 `min(换行数, lineClamp)` 行。未设置 `lineHeight` 时行高取宿主对单行的测量。`TextField` 不截断：单行横向滚动，多行纵向滚动。
- 文本的 min-content 宽度是最长单词，不是整段不换行宽度。定宽父级里的段落因此换行，而不会把父级撑出视口。
- 静态 JSX 文本按 HTML 折叠空白：连续空白变成一个空格，首尾空白去掉。表达式产生的字符串保持原样。
- 缓存 key：`(text, style, maxWidth, fontEpoch)`。
- 宿主注入字体时递增 `fontEpoch`，清空文本缓存。
- 三端换行/基线差异是接受的；内容避免依赖固定文本高度。

### 绘制

- 每个实例一个平台画布：iOS `UIView.draw` + CoreGraphics，Android `onDraw(Canvas)`，Web Canvas2D。
- 脏矩形 + 静态子树缓存；全量重绘作为兜底。
- `PaintCmd` 当前包含填充、文本、图片、裁剪、透明度、transform、内容层（`PushLayer` / `PopLayer`，tag 12/13）与滚动边缘虚化（`FadeEdges`，tag 11，八个 f32：x y w h top right bottom left）；图片按宿主可解析的 `src` 解码，离屏缓存。
- v1 不做模糊、滤镜、液态玻璃等高级合成。

### 命中与滚动

- 命中按绘制逆序，考虑 transform、clip、z-index。`display="none"` 与 `visibility="hidden"` 的子树不参与命中。`overflow` 为 `hidden` 或 `scroll` 的轴把命中限制在该节点 padding box 内；`visible` 轴在视口内不另加裁切。
- 指针捕获、手势识别在 UI 线程。
- 滚动由 UI 线程本地执行，不依赖脚本线程。横纵两轴各自维护偏移与范围；只有 `overflowX` / `overflowY` 为 `"scroll"` 的轴启用滚动。`"hidden"` 裁切且不可滚动，`"visible"` 不裁切也不滚动；这两种轴上 `ScrollTo` 归零。最大偏移是可滚动溢出超出 padding box 的部分：溢出矩形从 padding 边缘量起并已含末尾 padding，视口按 padding box 扣除，不再把 padding 从可视高度里减第二次。
- 滚轮/拖拽按实际命中路径逐轴消费 `(dx, dy)`：内层先 clamp，未消费部分在同次输入继续交给祖先；`overscrollX/Y={false}` 在对应轴截断剩余量。触摸识别时冻结该轴策略，wheel 每次读取最新已提交配置。
- `Shell::scroll_of` 返回 `(left, top)`；宿主分别通过 `bubble_scroll_x_of` / `bubble_scroll_y_of` 读取。
- 读屏滚动 action 复用同一 `scroll_path`：目标为语义节点最近的、接受该轴的可滚动祖先，步长为可见范围的 80%；无可用祖先或轴时返回错误，平台不得自行累加像素。
- 虚拟化列表协议：UI 线程计算可见区间并发送 `VisibleRangeChanged`；脚本创建/销毁 item 节点；未测量项用估算高度占位。滚动位置归 UI 线程，脚本只能 `ScrollTo`。

## 6. 故障与资源

- 脚本单事件软预算 32ms（仅埋点），硬超时 1s（中断）；初始值，可按实测调整。
- QuickJS 内存上限初始 64MB，栈上限单独设置；内存超限与硬超时走同一故障路径：中断执行、销毁 context、重建并重新挂载 bundle。
- 故障处理：上报 → 销毁 context → 重建 → 重新挂载 bundle → 错误占位与重试。应用状态丢失，内容级恢复。
- Web：一次调用返回前页面卡住；无法像 Worker 那样 `terminate()`。硬超时只存在于 native QuickJS。
- FFI 边界 `catch_unwind`，不允许 Rust panic 跨 C ABI；QuickJS 绑定用安全封装；fuzz 属 post-MVP。
- native 为进程内运行，没有强沙箱；依靠签名内容、内存配额、边界隔离降低风险。
- 配额：图片缓存、bundle 磁盘、网络并发；超限淘汰最旧资源。

## 7. 性能假设

- Taffy 布局 1k 节点应在数毫秒级；文本测量是主要成本，必须缓存。
- op 编码与跨线程传输应远小于布局/绘制成本；避免 JSON 与每帧大对象。
- QuickJS 解释执行足够处理事件与状态；重计算应放到宿主或预计算。
- CoreGraphics/Canvas2D 全表面重绘可能成为瓶颈；依赖脏矩形与静态缓存守住 60fps。
