# 08 · 共享变量与样式声明升级计划

状态：实现中。阶段 1（`expr.rs` 引擎 + VarTable + 四个状态源 + `ViewStyle`/`TextStyle` 组件与程序化 when/patch，`StateStyle` 整体移除）、阶段 2（JS 显式变量、`SetVar`、settle 事件、`bind` signal）、阶段 3（AnimSpec、tick 积分、双模写入、transform patch）与阶段 4（gesture 与 scroll 源）已落地。本文不引入任何新的验证工具或设备矩阵；所有验证仍限于 `cargo test --workspace`、`cargo check --workspace`、`node --test examples/web/*.test.mjs` 与手动 demo 冒烟。

实现落点：`crates/core/src/state_style.rs`（重写为声明表与求值）、`crates/core/src/props.rs`（`STATE_STYLE_PROPS` 按组件拆分）、`crates/core/src/shell.rs`（状态源采样、绘制合并、事件批次）、`crates/core/src/ops.rs` / `turn.rs`（新 op 与批次压缩）、`crates/core/src/expr.rs`（新：表达式引擎）、`crates/compiler/src/lib.rs`（JSX 属性表达式降级与组件属性白名单）、`crates/native/js/runtime.js`（JS 侧变量对象）、三端 ABI 不变（复用 `bubble_tick` / `bubble_frame`）。

## 1. 目标与边界

把状态样式从「两个硬编码状态位 + 静态 patch + 字符串条件」升级为「UI 线程共享变量 + 统一表达式引擎 + 显式样式组件」，为后续动画系统打地基：

- 状态是变量：pressed/focused/disabled/editing/滚动 offset/手势输出等 UI 线程状态暴露为变量实例，rust 直接驱动。
- 样式是表达式：样式声明的条件与 patch 值都编译为表达式程序，运行期由 UI 线程对变量求值，可连续跟随变量（如 `opacity = 1 - press`）。
- 与 signal 分工明确：signal 是脚本线程状态管理，触发绑定重跑与 op 批次；共享变量只活在 UI 线程，不触发任何脚本调度。两条单向通道：脚本 → UI 的 `SetVar` op（帧边界提交）；UI → 脚本的 settle 事件。

旧机制整体删除：`when` 字符串语法（`when="pressed focused"`）、`enabled` 属性、按最近父节点隐式解析、以及按 `pressed`/`focused` 硬编码的条件位。移除原因：字符串糖无法表达 disabled 与变量的组合逻辑，隐式目标解析会静默失效，条件位不可扩展。

明确不在本轮：布局属性绑定（width/position 等，会引入每帧 Taffy 重排）、JS 读回变量值、hover 与原始 pointer、惯性/回弹、性能基准（post-MVP backlog）。

## 2. 已确认的设计决策

| # | 决策点 | 结论 |
|---|---|---|
| 1 | 变量范围 | 完整原语：JS 可创建变量实例、关联状态源、直接写入（fire-and-forget） |
| 2 | 样式耦合深度 | 条件 + 连续取值绑定，patch 值可以是变量的函数，每帧由 rust 求值 |
| 3 | 编译期 computed | 谓词与数值表达式统一为一个表达式引擎（多变量 + 常量），编译期常量折叠；signal 值经 `SetVar` 通道接入 |
| 4 | 变量归属 | 实例句柄，不进命名注册表；JS 创建引用，可关联到具体状态源；也可创建无关联变量由 JS 驱动 |
| 5 | 动画语义 | 双模：默认即时写入，per-var 或 per-write 可选动画参数 |
| 6 | 可绑定属性 | 6 个 paint 属性 + transform（不进 Taffy、不触发重排） |
| 7 | settle 通知 | 复用现有 UI→脚本事件批次，新增 var-settle 事件 |
| 8 | 条件形式 | 全显式变量：`when` 只读作用域内变量；移除 `enabled`（`when={false}` 或绑定布尔变量覆盖） |
| 9 | 组件划分 | 两个组件：`ViewStyle`（backgroundColor/radius/opacity/fit/transform）、`TextStyle`（color/textAlign） |
| 10 | 目标解析 | 显式 `target` 属性，与变量关联同一风格，无隐式规则 |
| 11 | 首批状态源 | Pressed、Focused、Disabled（读现有 `node.disabled`）、Editing（读 EditSession） |

## 3. 当前实现证据

| 位置 | 现状 | 本计划影响 |
|---|---|---|
| `crates/core/src/state_style.rs` | `StateStylePatch` 是 6 个 `Option` 静态值；`when` 是 `u8` 位集合，只接受 `pressed`/`focused`；`enabled` 布尔门控 | 文件重写：声明的 `when` 与 patch 项为表达式程序；位集合、字符串解析、`enabled` 全部删除 |
| `crates/compiler/src/lib.rs` | `<StateStyle>` 不接受子节点、`when` 必填字符串、直接子节点拆分（`is_state_style`） | 拆为 `ViewStyle`/`TextStyle` 两个组件；属性白名单与表达式编译；无字符串 `when` |
| `crates/core/src/props.rs` | `STATE_STYLE_PROPS` 六属性单表 | 拆为两个白名单常量，分别属于两个组件 |
| `crates/core/src/shell.rs`：`state_bits` / `effective_paint_style` | 每帧读 `focused == id` 与 `visual.target == id` 两个位，再合并静态 patch | 位查询改为对 per-decl 缓存值的读取；位的产生下沉为状态源采样 |
| 同文件：`press_target` / `pressed_interest` / `focus_style_taken` | 用位集合做 O(声明数) 的快速判断 | 依赖编译期对程序的静态来源分析预计算 interest 位（第 7 节），命中路径不跑表达式 |
| 同文件：`node.disabled`（`Prop::Disabled`，`interaction_available`/焦点过滤已消费） | disabled 已参与交互门控 | `Disabled` 源直接读该字段，无新链路 |
| `crates/core/src/shell.rs`：`EditSession` | Field 编辑态在 UI 线程持有 | `Editing` 源读取编辑态 |
| `crates/core/src/turn.rs` | `SetStateStyleProp` 有批次压缩先例 | `SetVar` 复用同一压缩策略（同 var 保留最后一次写入） |
| `crates/core/src/shell.rs`：`advance_gestures` | UI 线程已有 tick 驱动骨架（`bubble_tick`） | 状态源采样与动画推进挂到同一 tick，不新增 ABI |
| `crates/core/src/shell.rs`：`take_gesture_events` + `crates/native/src/lib.rs`：`send_events` | UI→脚本事件批次已存在 | settle 事件作为新事件种类走同一条路 |
| `docs/02-runtime.md` op 表 | 4 条状态样式 op，value 为静态 `Value` | value 编码扩展出程序形式；op 变体更名（第 6 节） |

## 4. 共享变量（Var）

### 4.1 模型

变量是 UI 线程实例，脚本侧只有不透明句柄（id），没有命名注册表、没有名字冲突问题：

```
// UI 线程 VarTable
Var {
    id: u32,
    value: ExprValue,        // 当前值
    target: Option<ExprValue>, // 动画目标值（无动画配置时写入直接生效）
    anim: Option<AnimSpec>,  // per-var 默认动画
    source: Option<Source>,  // 关联的状态源；None = JS 驱动
    dependents: Vec<u32>,    // 依赖此变量的声明（求值索引）
}
```

`ExprValue` 是引擎的统一值类型，小型 tagged value：

- `Num(f32)`：opacity、radius、press 进度、scroll offset……
- `Color`：backgroundColor、color，参与 `mix()` 插值。
- `Bool`：谓词结果与布尔状态源。

离散绘制属性（`textAlign`、`fit`）不引入字符串值：用谓词 + `select(cond, a, b)` 在两个编译期常量之间选择。

### 4.2 状态源（Source）

关联即声明数据来源，写入权归 UI 线程驱动器，脚本对关联变量的 `set` 直接报错（fail fast）：

| Source | 产出 | 阶段 | 驱动方 |
|---|---|---|---|
| `Pressed(target)` | `Num` 0→1 进度；按下即 1，移出 `TAP_SLOP`/抬起/取消即 0；是否有过渡由该变量自身的 anim 配置决定，默认瞬时 | 1 | `track_press` / `reset_press` 现有路径 |
| `Focused(target)` | `Bool` | 1 | 现有 `focused == id` |
| `Disabled(target)` | `Bool`，读现有 `node.disabled` | 1 | `SetProp` 写 disabled 时 |
| `Editing(target)` | `Bool`，Field 编辑态 | 1 | `EditSession` 状态迁移 |
| `ScrollOffset(container, axis)` | `Num`，逻辑 px；`scroll.x` / `scroll.y` | 4 | 现有 Shell 持有的滚动 offset（`scroll_path` / `scroll_to` 后重采样） |
| `Gesture(decl, field)` | pan 的 `translationX/Y`、pinch 的 `scale/rotation`（角度连续展开，同 05 号文档约束）；源名 `gesture.translationX` / `gesture.translationY` / `gesture.scale` / `gesture.rotation`，一个源对应一个 field | 4 | 手势识别器的实时状态：pan 取 Accepted 候选的平移，pinch 取 transform state；手势空闲时 scale 为 1、其余为 0 |

关联在 `CreateVar` op 里一步完成（`source` 描述符含 target/decl 引用），不需要单独的 bind op。

disabled 节点上的 `Pressed` 源恒为 0：`interaction_available` 已在命中路径拦截，与今天 disabled 无按压反馈的行为一致。

### 4.3 生命周期

- id 由脚本侧 `Turn` 计数器分配（同 `next_state_style` 先例），`CreateVar` 随 op 批次提交。
- 无命名 → 无泄漏注册表；`DisposeVar` 由组件清理逻辑（owner/cleanup 语义）与样式组件销毁触发。
- `target_disposed(handle)` 级联：目标节点销毁时，关联到该节点的变量随之退役，依赖它们的声明标记为不匹配。
- 脚本对已 dispose 的句柄操作：报错（与手势声明现状一致）。

## 5. 表达式引擎（`crates/core/src/expr.rs`，新增）

### 5.1 编译产物

编译器把 JSX 属性表达式编译为紧凑寄存器式字节码，随 op 传输：

```
CONST value | LOAD var                          // 常量与变量读取
ADD SUB MUL DIV NEG                             // 数值
CMP LT LE GT GE EQ NE                           // → Bool
AND OR NOT SELECT                               // 控制流，SELECT 两分支同类型
MIX(a, b, t) | LERP(a, b, t) | CLAMP(v, lo, hi)
MIN MAX ABS | EASE(fn, t)                       // 内建函数（EASE 为动画期准备）
```

约束：

- 常量折叠在编译期完成；整棵程序折叠为常量时，直接退化为静态 patch 值（产出与今天等价的 op），运行期零求值——这就是编译期 computed 的形态。
- 编译期类型检查：`when` 必须折叠/推断为 Bool；patch 项类型必须匹配属性（`backgroundColor` 需要 Color，`opacity` 需要 Num，`textAlign`/`fit` 需要 `SELECT` 于枚举常量）。类型错误是构建错误，不是运行期错误。
- NaN/Infinity 防护与 05 号文档手势部分同一标准：除零、零距离插值不得产生非有限值。
- 字符串字面量、属性名、任意对象不进入引擎；引擎之外的表达式由编译器拒绝。

### 5.2 求值时机

- `VarTable` 维护 `var → dependent decls` 索引。变量值变化（源采样、动画 tick、`SetVar` op）只重算被牵连的声明。
- 每个声明缓存最近一次求值结果（`when` 布尔 + 各 patch 项值）。绘制合并只做合并，不求值——与今天 `overlay` 的形状一致，路径上不新增每节点开销。
- 求值全部发生在 UI 线程 tick 内（`advance_gestures` 同一入口扩展），结果变化置 `paint_dirty`。

## 6. 样式组件：ViewStyle / TextStyle

`StateStyle` 删除，替换为两个虚拟样式组件。两者共用同一运行时声明模型（同一张声明表、同一组 op），区别只在编译期属性白名单：

| 组件 | 可 patch 属性 | 类型 |
|---|---|---|
| `ViewStyle` | `backgroundColor` | Color |
| | `radius` | Num |
| | `opacity` | Num |
| | `fit` | `SELECT` 于枚举常量 |
| | `translateX` / `translateY` | Num（逻辑 px，绘制侧应用） |
| | `scale` | Num（节点中心缩放倍数） |
| | `rotate` | Num（节点中心弧度） |
| `TextStyle` | `color` | Color |
| | `textAlign` | `SELECT` 于枚举常量 |

声明模型（每组件同构）：

```rust
StyleDecl {
    target: u32,              // 显式 target 引用的节点句柄
    index: u32,               // 保持 OrderedDecls 序模型（同目标内按源码顺序，后者胜出）
    when: Program,            // Bool 程序（或折叠后的常量 true/false）
    patch: [Option<PatchSlot>; N],  // PatchSlot = 静态值 | Program；None = 未声明该项
    interests: u8,            // 编译期预计算：声明关注哪些状态源（第 7 节）
    resolved: ResolvedPatch,  // 最近一次求值缓存
}
```

op 协议沿用 4 条 op 的形状，变体更名以匹配新组件：`SpawnStateStyle`→`SpawnStyleDecl`、`SetStateStyleProp`→`SetStyleDeclProp`、`MoveStateStyle`→`MoveStyleDecl`、`DisposeStateStyle`→`DisposeStyleDecl`（协议号不变）。`SetStyleDeclProp` 的 value 编码扩展出程序形式（`when` 与各属性同一条路径）。批次压缩沿用现有 `SetStateStyleProp` 去重逻辑。

作者语法（无兼容层，直接新写法）：

```tsx
function Button() {
  const press = createVar({ source: "pressed", target: view });
  const focused = createVar({ source: "focused", target: view });
  const disabled = createVar({ source: "disabled", target: view });
  return (
    <View ref={view}>
      {/* 组合逻辑自由表达：disabled 优先，压过 pressed */}
      <ViewStyle target={view}
                 when={!disabled && press > 0}
                 backgroundColor={mix(baseColor, pressedColor, press)}
                 opacity={1 - press * 0.4} />
      <ViewStyle target={view}
                 when={disabled}
                 opacity={0.5} />
      <TextStyle target={label}
                 when={focused}
                 color={accentColor} />
    </View>
  );
}
```

- `when` 只接受表达式程序；无字符串形式、无 `enabled`。
- `target` 显式给出，与 `createVar` 的 target 参数同一风格；指向非存在/已销毁节点是构建期可查的引用错误。
- 标识符解析为作用域内的 `VarRef` 与编译期常量；解析不到即编译错误。
- 同目标多条匹配声明按源码顺序覆盖，后者胜出；语义与今天一致，但不再有「挂错目标静默失效」的歧义。
- 默认焦点框接管：目标上存在 `when` 当前为真且含 `Focused` 源的 `TextStyle`/`ViewStyle` 声明（含空 patch）时，接管默认焦点框——行为契约与今天一致，判断改为读兴趣位 + `resolved.when`。

## 7. 命中路径快速判断（不跑表达式）

`press_target` 在每次触点命中路径上运行，必须保持现在的 O(声明数) 位判断，不能对每个候选求值表达式：

- 编译期对每个程序做静态来源分析：程序是否可能读取各状态源（扫描 `LOAD var` 的变量源关联，经 `SELECT`/逻辑运算传播）。
- `pressed_interest(target)`、`focus_style_taken(id)` 改读预计算位；`focus_style_taken` 的「匹配中才算接管」仍需要求值，但只发生在焦点变化时，频率低。
- 保守规则：无法静态判定时按「可能关注」处理——宁多触发命中候选，不可漏反馈。运行期求值结果与兴趣位不一致是允许的（位只做预筛）。

`sync_press_target` 的「失去 press 反馈即放弃状态」逻辑改为读兴趣位 + 当前 `resolved.when`，行为契约不变。

## 8. 脚本接口与 signal 结合

### 8.1 JS 侧变量对象

```ts
// runtime.js 导出
const press = createVar({ source: "pressed", target: view });  // 关联：UI 驱动，只读
const t = createVar(0, { anim: { type: "spring", stiffness: 200, damping: 20 } });

t.set(1);                        // 双模：per-var 有默认动画则带动画
t.set(0, { anim: "instant" });   // per-write 覆盖为瞬时
await t.settle();                // settle 事件 → Promise（或 onSettle 回调）
t.bind(() => someSignal());      // signal 结合：render effect 重跑 → SetVar op
```

- `createVar` 在组件 setup 期调用，随 owner 清理销毁（同 cleanup 语义）。
- 关联变量：`set` 报错；无关联变量：`set` 产出 `SetVar` op 进脚本批次。
- `bind` 是脚本侧机制：signal 图的 render effect 把新值编码进 `SetVar`，帧边界提交——UI 线程永远不回调脚本，方向单向。
- 布尔源（focused/disabled/editing）也可由无关联变量 + `bind` 表达（如 signal 驱动的 disabled），与源变量在 `when` 表达式里地位相同。

### 8.2 settle 事件

- 仅对带动画的写入发出：`target ≠ value` 且收敛（到达 target 或被取消/覆盖）时产生 `VarSettle { var }` 事件。
- 走现有事件批次（`take_gesture_events` 同一条编码通道，`send_events` 分发），JS 侧 resolve 对应 `settle()` 的 Promise。
- 无动画写入不产生事件；被更新的写入（批次压缩后）只对最后一次产生事件。
- 阶段 2 实现（无动画）：每次 `settle()` 请求在批次应用时立即发 `BSVE { var }` 事件（独立于手势批次，`send_vars` 分发到 `__bubbleVar`），阶段 3 的动画 tick 在此基础上把「请求 → 发事件」改为收敛判定。

## 9. 动画系统接入点（为后续实现预留）

- `AnimSpec`：`tween { duration, easing }` 或 `spring { stiffness, damping }`；per-var 默认 + per-write 覆盖（决策 5 双模）。
- UI 线程 tick 扩展为：源采样 → 动画积分（`advance_animations(now_ms)`）→ 求值脏声明 → `paint_dirty`。变量收敛时产出 settle 事件。
- 动画是变量的属性，不是样式的属性：`mix(base, pressed, press)` 里 `press` 带动画，样式就自然获得过渡——声明方不感知动画存在。
- transform 加入绘制合并（translate/scale/rotate，绘制侧应用，不进 Taffy），无动画时与今天等价（单位变换零成本）。

## 10. 分阶段落地

| 阶段 | 内容 | 验证 |
|---|---|---|
| 1（MVP 内） | `expr.rs` 引擎 + VarTable + Pressed/Focused/Disabled/Editing 源 + `ViewStyle`/`TextStyle` 组件与程序化 when/patch（含静态折叠）；`StateStyle` 代码、语法、测试全部移除 | 已落地 |
| 2（MVP 内） | JS 显式变量、`SetVar`、settle 事件、`bind` signal | 已落地：`createVar(0)` + `set`/`bind`/`settle`，回归测试覆盖「写入 → settle」路径 |
| 3（MVP 内） | 动画系统：AnimSpec、tick 积分、双模写入、transform patch | 已落地：`createVar(0, { anim })` + `set(v, { anim })` 双模；tween（linear/easeInOut）与 spring（半隐式欧拉子步进）驱动 0..1 进度统一插值 Num/Color；收敛发 settle；`translateX`/`translateY`/`scale`/`rotate` 走 `PushTransform`/`PopTransform`（tag 9/10），三端绘制器接入 |
| 4（MVP 内） | 手势源（pan/pinch）与 ScrollOffset 源 | 已落地：`createVar({ source: "gesture.translationX", target: panDecl })` 等六个源；pointer 事件与滚动后重采样，声明随值重绘；native 回归覆盖 pan 跟随/抬手复位、pinch 缩放、scroll 混色 |

阶段 1 是替换而非兼容迁移：`crates/core/tests/state_style.rs` 契约测试随机制重写——位集合、字符串 `when`、`enabled`、隐式目标的测试用例删除；保留的是行为契约层断言（同目标按序覆盖、disabled 无按压反馈、focused 声明接管默认焦点框、`TAP_SLOP` 退出），以新语法重写。现有 demo 里的 `<StateStyle>` 用法同步改写为 `createVar` + `ViewStyle`/`TextStyle`。

明确延后（与第 1 节边界一致）：hover、布局属性绑定、JS 读回变量、惯性/回弹、下拉刷新、性能门禁与任何设备矩阵（post-MVP backlog，按 AGENTS.md 不规划）。

## 11. 风险

| 风险 | 缓解 |
|---|---|
| 程序字节码撑大 op 批次 | 常量折叠消除大部分程序；不做跨声明去重（收益小、复杂度高），批次体积监控留给 post-MVP |
| 静态来源分析漏判导致按压反馈失效 | 保守规则（第 7 节）：不可判定按「关注」处理；阶段 1 用重写后的契约测试锁定 `pressed_interest` 行为 |
| 显式 target 书写冗长 | `createVar` 与样式组件共用同一 target 引用，一次声明多处使用；不做隐式解析回退 |
| 每帧求值退化 | 依赖索引 + 声明级结果缓存；绘制合并保持纯合并；不引入跨帧 memo 之外的机制 |
| settle 事件风暴 | 仅动画写入产生；批次压缩后只报最后一次；无动画路径零事件 |
| `SELECT` 枚举类型检查漏网 | 编译期类型检查把 `textAlign`/`fit` 限定为「枚举常量或 SELECT 于枚举常量」，运行期再做一次性校验并 fail fast |
| 焦点框接管语义漂移 | 行为契约不变（目标上当前为真且含 Focused 源的声明接管，空 patch 也算）；阶段 1 重写对应测试 |
