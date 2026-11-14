# ADR: JS 打包策略

**状态**: 已实现  
**日期**: 2026-11-13  
**实现位置**: `crates/compiler` (Oxc transform), `crates/bundler` (rolldown), `crates/cli` (`bubble bundle/dev`)

## 背景

原有打包流程为"单文件直译 + 手写 manifest"。需要升级为支持多模块、TypeScript、资源导入和 Web HMR 的完整打包管线。

## 关键决策

### 1. Rust-only 打包管线

不引入 Node.js、npm 或 `node_modules`：

- **Oxc 0.158**: 一次 pass 只做两件事——去 TypeScript、降 JSX 为渲染调用
- **rolldown 1.3.2**: 负责模块图、链接、最终输出
- 任意 JS 语句、表达式和模块导出原样保留

### 2. 运行时边界

`bubble-shell` 和 `bubble:rt` 作为宿主提供的原生模块，不打进 bundle：

- Native: QuickJS loader 解析
- Web: import map 解析，两者指向同一份 `runtime.js`

发布产物只含应用代码，运行时独立演进。

### 3. 发布产物

单个 ES module + 资源 + manifest：

- 单 ESM，无 minify、无代码分割、无 QuickJS 字节码
- 资源命名：`assets/<name>.<hash8>.<ext>`
- bundle 内用相对路径引用资源
- source map 仅 dev 产物，release 不生成

### 4. Web HMR

开发态使用状态保留 HMR：

- rolldown `preserveModules` 输出每模块一个 CommonJS factory
- hot transform 识别组件、顶层 signal、`mount` 调用
- 浏览器 hot runtime 管理状态保留和模块替换
- WebSocket 推送更新

Native HMR 延后；当前 native 开发重新打包并重启。

### 5. 动态绑定识别

沿用现有规则：表达式中读取本地 `signal()` / `computed()` 调用即为动态绑定。

编译器在 JSX 属性表达式中识别这些调用，降为动态属性绑定 op。

### 6. 资源处理

- 支持 PNG/JPEG 图片导入
- 生成内容哈希命名文件，写入 `assets/`
- 导入语句编译为相对路径字符串
- manifest 记录资源列表及哈希、大小

### 7. 工具链

新增 `bubble` CLI：

- `bubble bundle <entry>`: 生产构建，输出 `<name>.json` + `<name>.js` + `assets/`
- `bubble dev <entry>`: Web 开发服务器，支持 HMR。页面用的 `sdk/web/src` 与 release `bubble_wasm.wasm` 由编译期工具链路径提供，不接受项目根目录参数。

fixture 继续使用多入口；不引入应用配置目录。

## 非目标

明确不在本阶段实现：

- TypeScript 类型检查（只去类型）
- JSX fragment、spread props
- SVG 资源导入
- native 状态保留 HMR 与 WebSocket dev host
- 动态 import 代码分割

## 结论

已实现并迁移所有 fixture。详细设计记录见 `07-js-bundling-plan.md`，实现偏差记录在该文档第 13 节。

参考文档 `02-runtime.md` 已同步编译管线摘要；冲突时以完整计划文档为准。
