# 05 · M2 前置：手势与滚动协商计划

状态：A-D 已实现。E 的可选协商 trace 已落地；模拟器已验收真实纵滑滚动与脚本挂起期间出帧，长按、双击、双指等设备交互未覆盖。本文记录 2026-11-11 确认的行为契约与实施顺序；字段名已落到实现。

实现落点：`crates/core/src/gesture.rs`（识别状态机）、`crates/core/src/shell.rs`（命中路径、逐轴滚动链、声明生命周期）、`crates/core/src/ops.rs`（BSOP v3 手势 op）、`crates/wasm` / `crates/native`（pointer/tick ABI）、`examples/web/runtime.js`（五种虚拟原语与事件分发）、三端宿主输入适配。Web 端已由 `examples/web/m0-smoke.mjs` 覆盖按下-抬起、事件分发与滚动。iOS/Android 模拟器记录见第 9.1 节。

## 1. 目标与边界

在 M2 输入控件之前，为应用组件和后续输入组件建立统一的 UI 线程手势识别与跨层协商机制：

- 声明方向、起始距离、失败条件、signal 驱动的启用条件。
- 支持按压反馈、单击、双击、长按、单指 pan，以及双指平移／缩放／旋转组合。
- 单指按轴协商，默认先满足条件者胜出；允许声明让祖先滚动等待。
- 触摸滚动与 wheel 默认逐轴级联，可按条件阻止外层滚动。
- Web、iOS、Android 共用 Rust 行为实现；Native 手势识别和内建滚动不等待脚本。

本轮不实现输入控件、文本选择、IME 或无障碍桥；它们使用后续 M2 计划。手势 API 面向应用作者与组件作者，平台适配仅负责原始输入归一化。

明确延后：公开原始 pointer 事件、hover、压力／倾角、独立 swipe、三指及以上手势、任意 waitFor／simultaneous 关系、UI 线程自定义视觉绑定、通用 transform 绘制、运行时惯性／回弹／下拉刷新，以及 Bubble surface 与宿主外层容器之间的滚动接续。

## 2. 当前实现证据

| 位置 | 当前行为 | 计划影响 |
|---|---|---|
| `crates/core/src/shell.rs`：`tap` | 命中节点或最近祖先的单个 handler，没有触点生命周期 | 建立命中路径、候选与识别状态，不能沿用按下即点击 |
| 同文件：`scroll_wheel` / `scroll_target` | 选一个有滚动范围的最深容器后 clamp；不检查请求方向上的剩余范围，也不传递剩余增量 | 改为实际命中路径上的逐轴消费链；文档中的方向剩余范围描述目前超前于代码 |
| `crates/core/src/turn.rs` | signal 与依赖图在脚本侧 `Turn`；`Shell` 持有 UI 树 | 条件经绑定求值和 op 提交，UI 不跨线程读取脚本 signal |
| `examples/web/runtime.js` | 原语注册、动态属性绑定、If/Each 的创建／移动／销毁已有；fragment 使用绝对定位零尺寸 View anchor | 复用生命周期与绑定；手势实体不创建 Taffy 节点，也不能成为可命中的 anchor |
| `crates/compiler/src/lib.rs` | JSX 属性可识别 signal 读取并降为动态绑定；没有对象表达式支持 | 使用虚拟 JSX 节点和标量属性，不为本方案额外引入对象声明语法 |
| `crates/native/src/lib.rs` / `script.rs` | `bubble_pointer_down` 立即发送 Press，脚本回调仅收到 tap | 替换内部输入与事件路径；`onPress` 仅保留公开语法糖 |
| iOS / Android / Web 示例宿主 | iOS 使用 UIKit pan，Android move 直接滚动，Web pointerdown 直接 tap，wheel 单独传入 | 三端接完整触点生命周期，由 Rust 统一识别，移除重复平台识别 |
| `docs/04-risks-roadmap.md` | M2 目前从编辑状态、输入代理、焦点与无障碍开始 | 增加本前置阶段作为输入控件实施门槛 |

本节记录实现前的源码与文档检查结论；实现后的验证见第 8、9 节。

## 3. 声明式 API

### 3.1 手势是虚拟子节点

```tsx
<View scrollY scrollChainY={locked() ? "contain" : "auto"}>
  <View>
    <PanGesture
      axis="x"
      direction="left"
      minDistance={12}
      failCrossDistance={8}
      enabled={editing()}
      reserveScroll="x"
      onStart={startDrag}
      onUpdate={updateDrag}
      onEnd={finishDrag}
      onCancel={cancelDrag}
    />
    <TransformGesture pan pinch rotate onUpdate={updateTransform} />
    <TapGesture onTap={activate} />
    <DoubleTapGesture onDoubleTap={zoom} />
    <LongPressGesture duration={500} maxDistance={8} onStart={beginSelection} />
    <Text>内容</Text>
  </View>
</View>
```

示例阈值是调用方配置示例，不是已定的运行时默认值。

- 虚拟节点绑定最近的实际 UI 祖先；组件、If、Each 和 fragment 不改变目标。
- 不创建布局盒，不绘制，不命中，不进入语义树，也不占实际 UI 子节点的布局索引。
- 拥有稳定 ID、目标 handle、声明顺序、配置与事件绑定，可独立创建、更新、移动、销毁。
- 同目标声明顺序由当前虚拟子节点顺序决定；一次触摸序列的候选顺序固定，调整顺序对下一序列生效。
- `<If>` 可条件创建／销毁手势。`enabled={editing()}` 通过现有 JSX 动态属性机制更新配置，不是 UI 线程 JS 谓词。
- 手势节点的属性、绑定、handler 与目标生命周期必须统一清理；不挂载到全局单例表后遗留。

公开类型：`TapGesture`、`DoubleTapGesture`、`LongPressGesture`、`PanGesture`、`TransformGesture`。不公开 PointerEvents 节点。

### 3.2 条件与冻结时机

| 配置 | 生效时机 |
|---|---|
| 已有候选的 enabled、方向、距离等识别条件 | 未胜出前读取最新已提交 UI 配置；在同一输入样本内使用同一配置快照 |
| 胜出手势的归属和配置 | 胜出后固定，直到 end 或 cancel；signal 更新不触发中途抢占 |
| 触摸滚动的 scrollChainX/Y | 该轴滚动识别时冻结；不因同次手势中的 signal 更新改变边界策略 |
| wheel 的 scrollChainX/Y | 每次输入读取最新已提交配置 |
| 当前触摸中新建的手势节点 | 下一次触摸序列起参与 |
| 当前触摸中销毁或改挂目标的手势节点 | 立即取消；规则见生命周期章节 |

序列开始时记录命中路径和已有声明身份，包括暂时 disabled 的声明；其后 enabled 变化不能复活已经失败的候选，也不能加入新建节点。UI 侧配置是最后应用的 op 快照，不能承诺反映尚未提交的脚本状态。

### 3.3 按压与点击

- `TapGesture` 提供 `onPressStart`、`onPressEnd`、`onPressCancel` 表达按压反馈；`onTap` 只在成功抬起后触发。
- 同目标可有 longPress / doubleTap；它们与 tap 使用专门组合规则，不作为互不相干的回调同时成功。
- 长按达到 duration 且未超过 maxDistance 才胜出；胜出后不再产生 tap 或 doubleTap。
- 只在最近点击目标上存在有效 `DoubleTapGesture` 时，单击等待双击窗口；没有则成功抬起立即触发。
- 成功双击只发送 doubleTap；窗口超时发送一次单击。第二次点击必须满足同目标、时间和空间条件；不匹配则按各自序列处理，不把不同目标的点击合并。
- 点击族只由最近的有效目标处理，不自动冒泡；祖先的 doubleTap 不延迟子按钮的 tap。
- `View onPress` 保留为隐式 `TapGesture onTap`；必须修正当前按下即触发的行为。与同目标显式 `TapGesture` 并存时报告配置冲突。
- 按压反馈不声明占轴，也不隐式冻结滚动；滚动或其他手势使点击失败时发送按压取消。
- 第二指加入时，相关 tap / longPress 失败，清理对应按压与计时状态。

点击时间窗、空间容差与手势默认距离在实现时统一调参；计时与超时判定在 UI 线程，不依赖 JS timer。

## 4. 跨层协商

### 4.1 候选、接受与失败

- 第一次按下按真实可见命中结果建立路径；不能因为前景节点没有可滚动范围就搜索到被遮挡的兄弟节点。
- pan 用逻辑坐标下从起点累计的位移判断 axis、direction、minDistance 与失败条件。方向指手指移动方向，区别于滚动 offset 增量。
- 候选尚未满足条件为 pending；匹配为 accepted；方向不匹配、超出失败距离或竞争失败后为 failed，本次序列不再复活。
- 每个输入样本先统一评估候选，再决定归属；不得用遍历时的副作用让较早扫描者意外获胜。
- 默认先满足条件者胜出；同样本同时满足时深层目标优先，同目标按声明顺序。阈值本身不表示优先级。
- 单指按轴分配消费权：横向 pan 可与祖先纵向滚动并行；同一轴不能被独立胜者重复消费。
- `axis="both"` 的 pan 获得两轴作为一个生命周期；单轴 pan 仅消费对应轴。pan 不自动拥有边界交接，滚动链有独立消费语义。

### 4.2 显式保留滚动轴

`reserveScroll="x" | "y" | "both"` 是局部关系：让命中路径上的祖先滚动等待指定轴上的当前候选，不让任意其他手势等待，也不提高该候选对自定义手势的优先级。

- 默认不保留。祖先先达到滚动阈值就能胜出，使冲突候选失败。
- pending 期间保存尚未消费的滚动增量；方向失败／位移超限／候选销毁／条件失效时解除等待。
- 释放时将等待期间累计的未消费增量一次交给滚动链，再继续处理后续增量；每段增量只能交付一次。
- 候选成功则按其占轴规则消费，不再把同段输入追补给滚动。
- longPress 也可显式保留；移动超过 maxDistance 失败并释放，达到 duration 则胜出。默认长按不阻塞滚动。
- tap/doubleTap 的点击等待不冻结滚动，不保留滚动轴。
- 保留配置必须具有确定的接受／失败条件，不能挂一个没有出口的永久 pending 候选。多个候选同时保留某轴时，在阻塞该轴的候选均释放后交付缓存，不能重复回放。

## 5. 双指 TransformGesture

选择组合识别，而非公开三个需要用户自行同步的独立识别器。

- `pan`、`pinch`、`rotate` 声明参与识别的分量；至少启用一个。
- 固定同一对触点，第二指加入时建立中心、距离、角度基线；共享一个 start/update/end/cancel 生命周期。
- 两指共同命中祖先路径上的已有 transform 声明参与识别；候选在第二指加入时固定，不能吸纳触摸期间新建的声明。
- 任一启用分量达到自身阈值即可识别；不要求平移、缩放、旋转同时达到阈值。
- 胜出后同时输出中心、累计 translation、scale、rotation 以及相应增量。角度必须连续展开，避免跨越正负 π 时跳变；距离退化时不得产生 NaN/Infinity。
- 可取消参与触点上已经开始的单指手势和触摸滚动，升级为 transform。这是“胜出后归属固定”的明确例外；此前已滚动距离不回退。
- 达到阈值前，既有单指滚动可继续；transform 基线仍是第二指加入时，不回溯撤销既有消费。
- transform 胜出后占用两根触点并阻止其 x/y 触摸滚动；平移、缩放、旋转可同时使用。
- 第三指不加入当前触点对；不利用第三指另起重叠手势。
- 任一配对触点抬起结束 transform；系统取消则 cancel。剩余手指不自动恢复单指拖动，需结束相关触摸后重新开始。

## 6. 默认滚动级联

公开响应式属性：

| 属性 | 值 | 含义 |
|---|---|---|
| scrollChainX / scrollChainY | `auto`，默认 | 该轴剩余增量交给下一个祖先滚动容器 |
| 同上 | `contain` | 在当前滚动容器截断该轴，丢弃剩余量，不继续传给祖先 |

- 触摸滚动和 wheel 共用逐轴消费函数，分别传入归一化增量及当前策略快照。
- 每次输入先由最深容器消费可用范围，实际消费量为 clamp 后 offset 的变化，剩余量同次继续向上；x/y 独立。
- 反向移动每次重新从最深容器开始，不永久转交祖先。
- 已在边界的 contain 容器仍截断；不能以“本次没有改变 offset”为由绕过。
- contain 是滚动边界策略，不代替 pan 的占轴；reserveScroll 是识别等待，不代替 contain。
- 未启用轴不能消费滚动，也不凭空成为该轴的滚动边界；滚动 offset 仍由 Shell 持有并 clamp，ScrollTo 继续遵守轴限制。
- 滚动链候选限于实际命中路径；拖动开始后固定路径，不随手指经过其他节点重新命中滚动目标。
- 返回实际消费、剩余和被边界截断的信息，不能仅以“是否移动”判断是否处理输入。
- wheel 归一化 deltaMode；平台传入的惯性 wheel 增量照常消费，但运行时不自行生成惯性。
- 不承诺向浏览器页面或原生外层容器交接残余增量。Web canvas 内触摸由 Bubble 管理，设置匹配的 touch-action 并接 pointer capture / pointercancel；避免浏览器与 Bubble 重复识别。

## 7. 线程、事件与生命周期

### 7.1 所有权

- UI 线程拥有触点表、命中路径、候选、阈值计时、归属、滚动 offset 和保留增量。
- 脚本线程拥有 signal 图、JS handler、虚拟声明的生产端状态；通过 op 更新 UI 配置，通过异步事件接收识别结果。
- 不引入 UI 条件表达式语言，不在 move 中同步调用 JS，也不以 JS preventDefault 决定归属。
- 自定义视觉由事件写 signal 驱动；脚本忙时可能延迟。UI 本地识别与内建滚动继续运行；Web 同线程脚本阻塞的既有限制保留。
- 新增状态都是实例内短期交互状态，不做持久化或磁盘迁移，不引入新的宿主能力或权限边界。

### 7.2 输入与事件契约

内部输入至少包含输入类型、pointer ID、pointer kind、主键／buttons、逻辑坐标与单调时间；支持 down/move/up/cancel，以及生命周期取消。鼠标主键与触笔可触发单指手势，非主键不误触点击。

手势事件至少携带实例 generation、序列／手势 ID、目标、阶段、时间和对应几何载荷；cancel 带原因。pan 的累计 translation 从按下位置起算，delta 为尚未交付的增量；transform 使用共享双指基线。end 包含最终累计状态与结束速度，不能依赖最后一个 update 恰好送达。

- start 与 end/cancel 等结构事件有序、不丢；每个已 start 手势恰好一个终结事件。
- 连续 update 可合并，但不能跨 start/end/cancel、跨手势或跨实例 generation 合并；合并保留最终累计值并正确累计 delta。
- 不默认公开逐点原始轨迹或 scroll 观察 API；后续需求不应反向改变当前消费语义。

### 7.3 移除与取消

- 活动手势声明被销毁或改挂目标：立即 cancel；已经消费的触点隔离到抬起，不向祖先重新交接。
- pending 声明被销毁：解除保留，按未消费增量回放规则继续协商。
- 新声明下一触摸序列参与；既有声明的 signal 配置可以更新 pending 条件。
- 目标被销毁、宿主失焦／暂停、系统取消、runtime 重建都清理触点、计时器、保留缓存和归属。
- 可用 handler 的活动序列必须在释放 handler 前交付终结通知；运行时已销毁时清理本地状态并丢弃旧 generation 事件，不向新脚本实例投递旧回调。
- 移除、取消与异步事件排队必须有统一的 handler 释放顺序，不能仅沿用当前立即删 handler 表项的方式。

## 8. 实施顺序

各阶段均属于后续实现，本次不执行。当前状态见下表。

| 阶段 | 状态 |
|---|---|
| A 输入和协议契约 | 已实现：BSOP v3、手势声明／配置／移动／销毁 op、`bubble_pointer`／`bubble_tick`、`pointer`／`tick` wasm 导出 |
| B 虚拟节点与 JSX 运行时 | 已实现：五种原语、If/Each 归属与清理、onPress 语法糖与冲突校验 |
| C Rust 识别和协商 | 已实现：单指、点击族、reserveScroll、双指 transform、结构事件与取消清理 |
| D 滚动消费与三端接入 | 已实现：逐轴链、auto/contain、三端触点生命周期。模拟器已证明真实滑动进入 `bubble_pointer` 并滚动；长按、双击、双指未在设备上覆盖 |
| E 验证与文档切换 | 进行中：核心、wasm smoke、可选协商 trace 与下方模拟器记录已覆盖；设备侧手势矩阵其余项未跑 |

### A. 输入和协议契约

涉及 `core` 输入／事件模块、`ops.rs`、`ir.rs`、`props.rs`、native C ABI/JNI、WASM 导出。

1. 定义带 ID 的完整 pointer 输入、wheel 输入与 UI 时钟驱动；明确坐标、时间单位及按钮映射。
2. 定义虚拟手势的创建／配置／移动／销毁协议及多类型事件绑定，替换内部单 handler 假设。
3. 采用现有二进制与 TLV 扩展约定；不兼容的 op 载荷升级版本，不能悄悄重解释旧数据。更新 JS、Rust、C/JNI/WASM 及所有生产者／消费者。
4. 协议拒绝未知枚举、非法数值和无效生命周期操作；ID 与事件附带实例边界，避免重建后的过期输入。

交付：所有平台可以送完整输入、登记声明并传递有类型的事件；不保留旧 pointerDown→立即 Press 的独立实现。

### B. 虚拟节点与 JSX 运行时

涉及 `examples/web/runtime.js`（native 也使用的 runtime）、编译器及绑定桥。

1. 注册五种手势虚拟原语，复用 JSX 属性动态绑定。
2. 区分 UI 节点索引与虚拟声明顺序；If/Each 创建、移动、销毁都能正确维护目标与声明生命周期。
3. 实现 onPress 语法糖与冲突校验；更新示例和调用方，以成功抬起后的点击语义驱动。
4. 取消通知、待投递事件与 handler 清理按统一顺序执行。

交付：不影响布局／命中，条件挂载可用，不要求应用引入 JS gesture 对象或 UI 线程 JS 谓词。

### C. Rust 识别和协商

1. 建立可注入单调时钟的识别状态机；先统一命中路径和候选，再评估与提交归属。
2. 完成单指按轴竞争、reserveScroll、失败回放、点击族计时与按压生命周期。
3. 完成双指 transform 和单指升级，覆盖触点对变化及角度／距离退化。
4. 完成结构事件顺序、update 合并与取消清理。

交付：相同输入轨迹在三端得到相同识别、归属和事件结果；关键状态转换不依赖 JS 是否及时执行。

### D. 滚动消费与三端接入

1. 将 Shell 的单目标 clamp 改为按真实命中路径逐轴消费，返回消费和剩余信息；接入 auto/contain、ScrollTo 与布局范围变化。
2. iOS 改送原始触点，移除 UIPanGestureRecognizer 的重复滚动路径；稳定映射 UITouch 到 pointer ID。
3. Android 处理 ACTION_POINTER_DOWN/UP、各 pointer ID、cancel 和密度换算，不再从单个 event.x/y 直接滚动。
4. Web 接 pointerdown/move/up/cancel、capture、touch-action 与 wheel 单位，移除 pointerdown 直接 tap。
5. 更新设备驱动中直接调用 pointerDown 模拟点击的路径，改为完整 down/up 或真实平台点击。

交付：同一 bundle 的按压、点击、长按、双击、按轴嵌套滚动及双指手势能在三端实际操作。

### E. 验证与文档切换

1. 完成下表的行为用例和平台实际交互；记录当前设备与环境限制。
2. 增加可选、默认关闭的本地协商 trace：候选 ID、目标、配置版本、pending/accept/fail 原因、占轴、回放与滚动消费；不在热路径默认生成字符串或原始轨迹日志。
3. 更新运行时事件契约、宿主输入契约、路线图和 API 示例；实现完成后再将本文从计划切换为实际行为说明。
4. 一次迁移内部所有调用方与旧事件协议；保留的 onPress 是正式语法糖，不保留旧按下触发路径。

## 9. 验证矩阵

| 层面 | 必须证明的行为 |
|---|---|
| 点击族 | 按下不触发 onPress；成功抬起触发；滚动／长按／第二指／取消不误触；双击存在才延迟；不同目标不合并；祖先双击不拖延子按钮 |
| 协商 | 先满足者胜出；同样本深层优先／同目标顺序；横 pan 与纵滚动并行；不重复消费同轴；明确失败后不可复活 |
| 保留 | 默认不保留；显式保留仅阻塞指定轴；长按可保留；失败追补且只追补一次；多候选释放不重复回放 |
| 滚动 | 内层部分消费＋祖先同次消费残余；反向回内层；x/y 分别路由；边界 contain 不泄漏；遮挡兄弟不被滚动；ScrollTo clamp 与布局缩小仍正确 |
| signal | pending 使用已提交新值；胜出后配置固定；两端提交顺序确定；新增节点不参加当前序列；移除 pending/active 的行为不同 |
| transform | 第二指阈值升级并取消单指；共享基线与分量；跨 π 连续；零距离稳定；第三指不入组；一指离开不恢复 pan |
| 生命周期 | 系统 cancel、失焦、暂停、目标销毁、声明移动、runtime 重建均清理；没有双终结、悬挂保留、失效 handler 或跨 generation 事件 |
| 事件传输 | start/update/end 顺序；合并 delta 和最终值正确；脚本忙时滚动继续、结构事件不丢；自定义视觉延迟符合已声明限制 |
| 平台 | 坐标单位与按钮一致；真实 down/up、拖动、长按、双击和双指操作；iOS 无重复识别，Android 多 pointer 正确，Web capture/cancel 与 wheel 单位正确 |
| 虚拟树 | 手势不影响布局与命中；If/Each 透明归属、顺序与销毁正确；onPress 语法糖冲突可诊断 |

Rust 使用确定性输入轨迹和假时钟覆盖状态机；跨线程／runtime 集成用例覆盖已提交 signal、事件合并及清理。三端需要实际交互证明，不能仅以 headless 接口调用代替平台适配验收。新增永久测试只断言消费者可见行为和状态不变量，不锁定源代码文本或私有实现细节。

### 9.1 模拟器记录（2026-11-11）

命令：`cargo run -q -p bubble-eval -- ios` 与 `cargo run -q -p bubble-eval -- android`。两份报告 `ok: true`。

| 设备 | 交互 | 结果 |
|---|---|---|
| iOS 27.2 Simulator，iPhone 18 Pro（`85111F63-1CD1-4C6F-8333-6039F90CD1A5`） | XCUITest `press` 0.05s 后从 (0.5, 0.75) 拖到 (0.5, 0.35)，经 `touchesBegan/Moved/Ended` → `bubble_pointer` | `scroll offset` 45.3（≥40）；`frames during hang` 60；帧均 16.87 ms；paint 2267 B；elapsed 4512 ms |
| Android 16 模拟器 `emulator-5554`（`sdk_phone64_arm64`，arm64-v8a） | `adb shell input swipe` 纵滑 300 ms，经 `onTouchEvent` → `BubbleNative.pointer` | `scroll offset` 55.6（≥40）；`frames during hang` 61；帧均 16.83 ms；paint 2267 B；elapsed 1510 ms |

两端共同通过、但不是手势矩阵：脚本死循环期间宿主继续出帧并在故障后重挂载（`remount text` = `Count: 0`，`faults` = 1，`font epoch` = 1）。按钮点击由宿主在第 20 帧直接调用 `bubble_pointer` down/up，不是平台注入的触点。

本次没有在设备上覆盖：长按、双击、双指 transform、横纵并行、`reserveScroll` 回放、`scrollChain` contain、声明销毁取消、失焦/暂停清理。这些仍只由 Rust 轨迹测试和 Web smoke 证明。

## 10. 风险、调参与发布

已接受的权衡：

- reserveScroll 失败回放会在释放时造成可见位移跳变；这是保留完整移动距离的选择，需要交互验收。
- first-ready 意味着更高 minDistance 的子手势可能输给滚动；组件必须显式 reserveScroll 才能要求祖先等待。
- 双指升级不撤销先前滚动，阈值前的列表位移可能可见。
- signal 在提交之前不可见，胜出后冻结；不能把实时 JS 条件误当 UI 同步条件。
- 自定义视觉依赖 JS；本轮只保证 Native 本地识别与内建滚动，不承诺自定义变换在脚本卡顿时仍跟手。
- onPress 从按下改为成功抬起，是有意的行为修正；所有示例、驱动和契约测试必须随之迁移。

已确定的默认值：点击 slop 10，双击窗口 300 ms／空间容差 24，长按 500 ms／最大位移 10，pan 最小距离 10，触摸滚动起始阈值 8，pinch 阈值 0.05，rotate 阈值 5°。点击族与长按的计时由宿主每帧调用 `bubble_tick`／`tick` 推进。

可选协商 trace 默认关闭；`Shell::set_gesture_trace` 开启后记录候选接受／失败、预留缓存、回放与滚动消费，`take_gesture_trace` 取走文本。wasm 导出 `set_gesture_trace`／`take_gesture_trace`，native 导出 `bubble_set_gesture_trace`／`bubble_take_gesture_trace`。触点到候选、保留缓存和事件队列目前不设硬容量上限；待真机压力测试确定数值，不改变正常路径语义。

发布按内部协议和三端宿主共同切换，不做旧按下点击行为的兼容分支。完整交互验收前不进入 M2 输入控件开发；出现问题回退对应 runtime／宿主／bundle 的一致版本，不在同一实例混用新旧输入协议。没有持久化状态迁移。
