# ADR: 自绘输入模型

**状态**: 已实现  
**日期**: 2026-11-12  
**实现位置**: `crates/core/src/editing.rs`, `crates/core/src/layout.rs` (`LayoutSnapshot`)

## 背景

原有文本编辑实现存在 Android IME 桥接的语义缺口：

- UTF-16 选区记录，但组合更新时光标固定在组合末尾；组合期间拒绝选区变化
- `InputConnection` 忽略 `newCursorPosition`，`deleteSurroundingText` 转成重复退格
- `composing` / `composingText` 在 Kotlin 和 Rust 两处维护，可能分叉
- 编辑几何由前缀测宽推导，与 Android `StaticLayout` 不一致
- BSED v1 每次携带完整字符串，Android 单命令逐次调用导致多次桥接

目标：修复语义缺口，同时保留完全自绘的文本、光标和选区。Android `InputConnection` 只接入系统 IME，不使用 `EditText`。

## 关键决策

### 1. 权威状态与编辑会话

- Rust UI 线程拥有唯一权威状态：文档、方向性选区、组合范围、revision、撤销历史
- 平台适配器通过会话读取原子快照并提交编辑事务
- Android 不维护第二份文档；若需 `Editable` 对象，只能是会话投影
- 会话有明确的开始、关闭和失效语义；焦点切换、字段销毁使旧会话失效
- 每次提交校验会话身份和期望 revision，返回成功/冲突/失效

### 2. 文档位置与编辑事务

定义可扩展的文档位置，而非暴露 UTF-16 索引：

- 平台 IME 边界使用 UTF-16 code unit offset；转换时校验边界，不能拆代理对
- 选区保存 anchor/active 端点，保留方向；光标是折叠选区
- 组合范围独立于选区；选区可位于组合内，IME 可在组合中移动/替换/删除
- 事务：范围替换、设置选区、设置/更新/结束组合、删除范围、撤销/重做、reset
- 替换正确应用 IME 的 `newCursorPosition`
- 文档位置预留 affinity，为后续富文本预留插入点归属定义

### 3. 组合与应用写入隔离

- IME 命令是活动组合的唯一编辑者
- 应用 `SetEditingValue` 在组合期间返回 composition conflict，不应用
- 应用若需接管，必须显式 reset：结束并取消组合，使旧快照失效
- 避免 Flutter Android 的"应用写入悄悄结束组合"问题

### 4. 平台布局快照 (LayoutSnapshot)

替换前缀测宽推导，使用平台真实布局结果：

- 失效键：revision、可用宽度、字号、字重、行高、对齐、密度
- Android `StaticLayout`、iOS Core Text、Web canvas/DOM 测量各自生成 BLS1 快照
- 绘制与 IME 查询共用快照
- 旧 revision 不能驱动光标、命中或选区
- `text_metrics` / `hit_unit` 从编辑路径删除

### 5. BSED v2 协议

- 增量传输：只发送变化的字段，不重复完整字符串
- 支持同一批次多个事务
- 携带 revision、会话身份、期望前置条件

## 结论

阶段 A-F 已实现。验收以 `cargo test --workspace` 和 `node --test sdk/web/*.test.mjs` 为准。

模拟器 harness 已随 `examples/` 删除，不作为 MVP 门槛。视觉选区手柄拖动、实体设备与厂商键盘差异进入 post-MVP backlog。

三端 IME 适配迁移完成，统一使用编辑事务语义。
