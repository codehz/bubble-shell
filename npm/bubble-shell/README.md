# bubble-shell

Build and serve Bubble UI bundles.

## Install

```sh
npm install -D bubble-shell
```

Published platforms: macOS ARM64, Linux x64, Windows x64. npm installs the matching `@bubble-shell/*` package as an optional dependency. The `bubble` command is the package bin.

## Commands

```sh
bubble bundle <entries...> --out <dir> [--id <id>] [--dev]
bubble dev <entry> [--port <port>]
```

`bubble bundle` writes `<name>.js`, `<name>.json`, and hashed assets. `bubble dev` serves a canvas host with hot updates (default port 4173). Web host scripts and the wasm shell are embedded in the binary.

`import … from "bubble-shell"` stays an external specifier after compilation. The dev server and the host runtime provide the implementation. Do not import a runtime from this package.

## TypeScript

Types ship as `index.d.ts` (`package.json` `"types"`). No `@types` package. A project that depends on `bubble-shell` resolves `signal`, `mount`, `ViewProps`, and the rest through that file.

## Troubleshooting

- **Unsupported platform.** `darwin-arm64`, `linux-x64`, and `win32-x64` are the published targets. Other `process.platform` / `process.arch` pairs exit with `Unsupported platform`.
- **Missing platform package.** Optional dependencies are skipped when the install platform does not match, or when the install used `--omit=optional`. Reinstall without omitting optional dependencies.
- **Permission denied.** The Unix binary must be executable (`chmod +x` on `bin/bubble` inside the platform package). npm keeps the mode from the published tarball.
- **Offline install.** The platform package has to be in the npm cache or the lockfile install set. An offline install on a machine that never downloaded `@bubble-shell/<platform>` cannot fetch the binary.
