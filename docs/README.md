# Bubble Shell 架构文档

可嵌入 web / Android / iOS 宿主的自绘 UI 运行时：远程下发签名内容包，提供带交互的动态界面，不依赖 Web 渲染。

## 文档

| 文档 | 内容 |
|---|---|
| [01-overview.md](01-overview.md) | 目标、边界、总体架构、关键决策、验收指标、术语 |
| [02-runtime.md](02-runtime.md) | 编译管线、bundle、op 协议、双线程、布局/绘制/文本、故障处理 |
| [03-embedding.md](03-embedding.md) | 宿主能力桥、嵌入生命周期、导航、无障碍、文本输入、更新、可观测性 |
| [04-risks-roadmap.md](04-risks-roadmap.md) | 风险、v1 非目标、里程碑、测试策略、待定项 |

## 状态

- 版本：v1 架构草案
- 日期：2026-11-11
- 前提：第一方内容、单实例嵌入、native QuickJS + Web Worker、平台文本渲染
