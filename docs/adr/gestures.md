# ADR: 手势与滚动协商

**状态**: 已实现  
**日期**: 2026-11-13  
**实现位置**: `crates/core/src/gesture.rs`, `crates/core/src/shell.rs`

## 背景

在 M2 输入控件之前，需要为应用组件和后续输入控件建立统一的 UI 线程手势识别与跨层协商机制。原有实现存在以下问题：

- 按下即点击，没有触点生命周期和状态机
- 滚动只选一个容器 clamp，不检查方向剩余范围，不传递剩余增量
- 各平台重复识别手势（iOS UIPanGestureRecognizer、Android 直接滚动、Web pointerdown 直接 tap）

## 关键决策

### 1. 虚拟手势节点

手势声明作为虚拟子节点，绑定最近的实际 UI 祖先：

```tsx
<View>
  <TapGesture onTap={activate} />
  <PanGesture axis="x" onPanStart={startDrag} />
  <Text>内容</Text>
</View>
```

- 不创建布局盒，不命中，不进语义树
- 拥有稳定 ID、目标、声明顺序、配置和事件绑定
- 生命周期随目标节点统一清理

### 2. UI 线程识别与协商

- Web、iOS、Android 共用 Rust 状态机
- 按轴竞争：默认先满足条件者胜出；横向 pan 与纵向滚动可并行
- 条件在未胜出前读取最新配置；胜出后固定，直到 end 或 cancel

### 3. captureAxis 保留机制

`captureAxis="x" | "y" | "both"` 让祖先滚动等待当前候选：

- pending 期间保存未消费增量
- 失败时将累计增量一次交给滚动链
- 成功则按占轴规则消费，不再追补滚动

### 4. 逐轴滚动级联

`overscrollX` / `overscrollY` 控制边界策略：

- 触摸滚动和 wheel 共用消费函数
- 最深容器先消费可用范围，剩余量同次向上
- `false` 截断该轴，不传给祖先（contain）
- x/y 独立路由，反向移动回到内层

### 5. 双指 TransformGesture

组合识别 pan/pinch/rotate：

- 第二指加入时建立基线，共享生命周期
- 任一启用分量达到阈值即可胜出
- 可取消单指手势并升级为 transform
- 角度连续展开，避免跨 π 跳变

## 结论

已实现并完成文档切换。验收以 `cargo test --workspace` 和 `node --test examples/web/*.test.mjs` 为准；模拟器 harness 保留为可选回归入口，实体设备和厂商键盘差异进入 post-MVP backlog。

公开类型：`TapGesture`、`DoubleTapGesture`、`LongPressGesture`、`PanGesture`、`TransformGesture`。

移除了 `View onPress` 隐式手势；点击统一使用显式 `TapGesture onTap`，并在成功抬起后触发。
