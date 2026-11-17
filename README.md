# Bubble Shell

[![npm](https://img.shields.io/npm/v/bubble-shell)](https://www.npmjs.com/package/bubble-shell)

可嵌入 web / Android / iOS 宿主的自绘 UI 运行时：用 TSX 写界面，编译成远程下发的签名内容包，宿主内渲染与交互，不依赖 Web 渲染。

架构与设计文档见 [docs/README.md](docs/README.md)。本文是开发者的构建与使用指南。

## 安装

```sh
npm install -D bubble-shell
```

安装后提供 `bubble` 命令（`bubble bundle`、`bubble dev`）。已发布平台：macOS ARM64、Linux x64、Windows x64。类型定义随包发布，无需额外 `@types`。

`import … from "bubble-shell"` 在编译后保持为 external specifier，由 `bubble dev` 或宿主运行时提供实现，不从 npm 加载运行时。

## 环境要求

- Rust stable（含 `wasm32-unknown-unknown` target：`rustup target add wasm32-unknown-unknown`）
- Node.js ≥ 20.6（Web 单测与 smoke 使用 `node:module` register hooks）

## 构建

workspace 的 `default-members` 是 `crates/cli`，所以裸 `cargo run` / `cargo test` / `cargo check` 只构建宿主侧 CLI，不会碰 wasm（`bubble-wasm` 是 wasm32 cdylib，为宿主目标链接必然失败）。

```sh
# 构建 wasm shell。CLI 编译期嵌入这份 wasm，须先于 bubble-cli 构建。
# `bubble dev` 与 web 单测都依赖它。
cargo build -p bubble-wasm --target wasm32-unknown-unknown --release
# 产物：target/wasm32-unknown-unknown/release/bubble_wasm.wasm
```

## 开发调试（dev server）

```sh
cargo run -- dev examples/gallery/app.tsx
# bubble dev on http://127.0.0.1:4173 (…/examples/gallery/app.tsx -> app.js)
```

- 在浏览器打开终端打印的地址，页面在 `<canvas>` 上渲染应用，`/` 路由附带 import map 与 WebSocket 热更新客户端。
- 需要先完成上面的 wasm 构建，再编译 CLI。`bubble dev` 从二进制内嵌的 wasm 与 web 宿主模块提供 `/bubble.wasm` 和 `/src/*.js`，不读取仓库里的 `sdk/web/src` 或 `target/`。
- 修改入口及其依赖的源文件会自动重编译，并通过 WebSocket 推增量更新；删除文件或改坏构建时推整页 reload / 错误信息。
- `--port <port>` 可改端口（默认 4173）。

可运行的入口示例：

- `examples/gallery/app.tsx` — gallery 演示
- `crates/testing/apps/*.tsx` — fixture 应用（`counter`、`editing`、`gestures`、`focus`、`slot`、`media`）

## 打包（bundle）

```sh
cargo run -- bundle <entries...> --out <dir> [--id <id>] [--dev]
```

输出到 `<dir>`：

- `<name>.js` — 应用 ESM 入口（Oxc 降类型/JSX + rolldown 单 ESM）
- `<name>.json` — manifest（schema、入口、文件哈希、平台列表；签名当前为 M0 占位）
- 资源按 `<name>.<hash8>.<ext>` 命名落盘

多个 entry 一起打包时 `--id` 只对单个 entry 有效。`--dev` 产出开发态 bundle。

## 测试与 smoke

```sh
cargo test --workspace      # 离线、无需设备
cargo check --workspace
node --test sdk/web/*.test.mjs   # 需要先构建 wasm（见上）
node sdk/web/m0-smoke.mjs counter   # 手动 smoke：渲染、点击；gestures 走手势路径
```

`node --test` 与 smoke 通过 `target/` 下的 wasm 构建产物和 in-process bundle 运行，不生成需要提交的文件。

## 目录速览

| 路径 | 内容 |
|---|---|
| `crates/core` | 保留树、布局、手势、编辑、语义、绘制指令 |
| `crates/compiler` | TSX/TS/JS → 原生渲染调用 |
| `crates/bundler` | Oxc + rolldown 打包、资源处理、dev 构建 |
| `crates/cli` | `bubble bundle` / `bubble dev` |
| `npm/` | `bubble-shell` 与 `@bubble-shell/*` 平台包模板；二进制与 `index.d.ts` 由发布流程填入 |
| `crates/wasm` | wasm shell（Web 宿主用） |
| `crates/native` | native C ABI + QuickJS 脚本运行时 |
| `crates/testing` | fixture 应用与 headless 驱动脚本 |
| `sdk/web` | Web 宿主（canvas 回放、a11y、IME、文本布局）与 Node 单测 |
| `sdk/ios` / `sdk/android` | iOS / Android 嵌入层 |
| `examples/gallery` | 演示应用 |

## 文档

- [docs/README.md](docs/README.md) — 架构文档索引
- [docs/reference/overview.md](docs/reference/overview.md) — 目标、边界、总体架构
- [docs/reference/runtime.md](docs/reference/runtime.md) — 编译管线、op 协议、双线程
- [docs/reference/embedding.md](docs/reference/embedding.md) — 宿主嵌入
- [docs/roadmap.md](docs/roadmap.md) — 里程碑与当前状态

iOS / Android 宿主 App 目前不在仓库内；重建时按 workspace crate 形式提供（见 `AGENTS.md`）。
