# Bubble Shell 架构文档

可嵌入 web / Android / iOS 宿主的自绘 UI 运行时：远程下发签名内容包，提供带交互的动态界面，不依赖 Web 渲染。

## 文档结构

### 参考文档 (reference/)

稳定的架构与实现参考：

- [overview.md](reference/overview.md) — 目标、边界、总体架构、关键决策、验收指标、术语
- [runtime.md](reference/runtime.md) — 编译管线、bundle、op 协议、双线程、布局/绘制/文本、故障处理
- [embedding.md](reference/embedding.md) — 宿主能力桥、嵌入生命周期、导航、无障碍、文本输入、更新、可观测性

### 架构决策记录 (adr/)

已实现的关键设计决策：

- [gestures.md](adr/gestures.md) — 手势与滚动协商：虚拟节点、UI 线程识别、按轴竞争、captureAxis、逐轴滚动级联
- [input-model.md](adr/input-model.md) — 自绘输入模型：编辑会话、事务协议、平台布局快照、Android IME 语义修复
- [js-bundling.md](adr/js-bundling.md) — JS 打包策略：Oxc + rolldown、单 ESM、运行时边界、Web HMR

### 实现中的计划 (plans/)

活跃的设计与实施文档：

- [shared-variables.md](plans/shared-variables.md) — 共享变量与样式声明升级：移除 StateStyle，改为 ViewStyle/TextStyle + UI 线程变量 + 表达式引擎

### 风险与路线

- [risks.md](risks.md) — 风险表、v1 非目标、延后验证 backlog、测试策略、基础能力设计缺口、待定项
- [roadmap.md](roadmap.md) — MVP 范围与验收、里程碑（M0/M1/M2/M3）、当前状态

## 快速索引

**我想了解...**

- 总体目标和边界 → [reference/overview.md](reference/overview.md)
- 编译管线和 op 协议 → [reference/runtime.md](reference/runtime.md)
- 如何嵌入宿主应用 → [reference/embedding.md](reference/embedding.md)
- 手势识别如何工作 → [adr/gestures.md](adr/gestures.md)
- 文本输入如何处理 → [adr/input-model.md](adr/input-model.md)
- JS 打包流程 → [adr/js-bundling.md](adr/js-bundling.md)
- 当前开发进度 → [roadmap.md](roadmap.md)
- 风险与缓解策略 → [risks.md](risks.md)

## 当前状态

- **版本**: v1 架构草案
- **日期**: 2026-11-14
- **阶段**: MVP 优先
- **构建工具**: `bubble bundle` / `bubble dev`（Oxc + rolldown 单 ESM）
- **活跃计划**: 共享变量与样式声明升级（[plans/shared-variables.md](plans/shared-variables.md)）
- **验收**: 核心测试 `cargo test --workspace` + Web 单测 `node --test sdk/web/*.test.mjs`

实体设备、真实读屏软件、性能门槛与故障演练进入 post-MVP backlog。
