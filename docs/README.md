# Bubble Shell 架构文档

可嵌入 web / Android / iOS 宿主的自绘 UI 运行时：远程下发签名内容包，提供带交互的动态界面，不依赖 Web 渲染。

## 文档

| 文档 | 内容 |
|---|---|
| [01-overview.md](01-overview.md) | 目标、边界、总体架构、关键决策、验收指标、术语 |
| [02-runtime.md](02-runtime.md) | 编译管线、bundle、op 协议、双线程、布局/绘制/文本、故障处理 |
| [03-embedding.md](03-embedding.md) | 宿主能力桥、嵌入生命周期、导航、无障碍、文本输入、更新、可观测性 |
| [04-risks-roadmap.md](04-risks-roadmap.md) | 风险、v1 非目标、MVP 范围与验收、延后验证 backlog、测试策略 |
| [05-gestures-plan.md](05-gestures-plan.md) | M2 输入控件前置计划：虚拟手势节点、声明式协商、双指 transform、按轴滚动级联与行为契约 |
| [06-input-model-plan.md](06-input-model-plan.md) | 自绘输入模型重设计：编辑事务、Android IME、跨平台迁移、排版快照与行为契约 |
| [07-js-bundling-plan.md](07-js-bundling-plan.md) | JS 打包：Oxc + rolldown、原生运行时模块、资源命名、Web HMR；设计与迁移步骤，实现偏差见第 13 节 |

## 状态

- 版本：v1 架构草案
- 日期：2026-11-13
- 阶段：MVP 优先；JS 打包已按 [07-js-bundling-plan.md](07-js-bundling-plan.md) 切到 Oxc + rolldown 单 ESM，`bubble bundle` / `bubble dev` 是构建入口。实体设备、真实读屏、性能与故障演练进入 post-MVP backlog，不作为当前验收。
