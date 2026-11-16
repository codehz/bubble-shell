# Bubble Shell · Agent Rules

## Milestone: MVP first

The current goal is a runnable demo, not a verification program. Finish the MVP
before adding any expensive validation.

MVP definition:

- The wasm shell runs the shared fixture apps: clicks, scrolls, text edits,
  and focus moves are exercised by `node --test sdk/web/*.test.mjs` and the
  manual smoke `node sdk/web/m0-smoke.mjs counter|gestures` (wasm must be
  built first: `cargo build -p bubble-wasm --target wasm32-unknown-unknown --release`).
- iOS and Android host apps launch the same fixtures on a simulator and
  perform one basic click/scroll/edit. Host apps are workspace crates (like
  nsh's `crates/*-host`); they are not in the repo yet and never live under
  `examples/`.
- `cargo test --workspace` stays offline and device-free.
- `node --test sdk/web/*.test.mjs` passes.

Deferred until after the MVP. Do not add these, and do not stop a product change because a plan mentions them:

- Physical-device matrices, vendor keyboards, external keyboards.
- Real screen-reader acceptance (VoiceOver, TalkBack, NVDA) and accessibility
  trees captured from a device.
- First-frame/FPS/memory/bundle-size gates, performance profiling baselines.
- Update rollback drills, fault injection, fuzz, golden/screenshot tests,
  cross-platform conformance suites, acceptance matrices.
- Device reports, recorded verification runs, screenshots, or xcresult
  attachments.

Simulator checks already in the repo may be run. They are optional regression
entry points, not MVP gates. Do not add new ones unless the user asks.

## Verification budget

Run existing tests. No approval needed. Use the narrowest command that covers the change:

```sh
cargo test --workspace     # offline, no devices
cargo check --workspace
node --test sdk/web/*.test.mjs
```

Also fine: the wasm smoke (`node sdk/web/m0-smoke.mjs counter|gestures`, after
`cargo build -p bubble-wasm --target wasm32-unknown-unknown --release`), and an
iOS or Android simulator launch that is already wired up. Tap, scroll, or type
once when that is what the change affects.

Do not add tests, fixtures, harnesses, or CI jobs. In particular, do not add:

- XCUITest, UiAutomation, `adb shell input`, Appium/Detox, or any new simulator
  or device automation harness.
- VoiceOver / TalkBack / NVDA acceptance runs.
- Vendor keyboard, physical-device, or real-screen-reader matrices.
- Golden/screenshot tests, fuzzing, fault-injection suites, performance/FPS/
  memory gates, and CI jobs that boot emulators or devices.
- Verification-only crates, acceptance-report harnesses, fixture matrices, and
  multi-host acceptance suites.

Running a command that already exists is not one of the items above.

## Demo hosts stay clean

Demo host apps are not in the repository right now. When they return they are
workspace crates (like nsh's `crates/*-host`), never projects under
`examples/`, and the rules below apply to them:

- Default app launch must render and interact normally. Acceptance/self-test
  phases must be disabled unless an explicit environment variable or intent
  extra turns them on.
- Never embed auto-run tap/hang/report sequences in the default launch path.
- Verification-only branches must be removable and must not affect normal
  behavior.

## Writing tests

- Do not add tests unless the user asks. Running the existing suite is the check.
- Test observable behavior, not source text, wiring, forwarding, or incidental
  defaults.
- Do not duplicate the same path in several suites.
- Do not add a test matrix for platform variants that cannot run locally now.
- Do not keep tests for deleted features or abandoned protocols.
- No property-test, fuzz, or generated-matrix requirements in the MVP.

## Writing code

- No speculative guards, retries, fallbacks, validation layers, telemetry, or
  recovery paths without an observed failure or an explicit request. Fail fast.
- No compatibility shims, legacy aliases, or dual protocol paths unless asked.
- Do not extract helpers solely so a test can reach private logic.
- Prefer deleting code over adding a layer.
- Platform-specific code is acceptable only for the demo targets that ship now.
- Do not add defensive code just to satisfy a hypothetical acceptance run.

## Commit messages

Subject is `area: description` — an area, a colon, a space, an imperative
description. No Conventional-Commit type prefixes (`fix:`, `feat(core):`,
`refactor(x):`).

- Area is lowercase, short, and names the slice the change touches: crate
  (`core`, `compiler`, `wasm`, `ffi`, `native`), sdk surface (`web`),
  fixture (`gallery`), feature (`textfield`, `gestures`, `worklets`), or
  build scope (`workspace`). Cross-cutting fixes pick the dominant slice.
- Description states what the change does, not what was wrong:
  `core: hit-test through node paint transforms`, not `fix: hit-test bug`.
- Optional body: blank line after the subject, then free-form paragraphs or
  `- ` bullets. Bodies explain scope, API/protocol changes, and test notes;
  do not restate the subject.

## Documentation

- Do not record verification runs, screenshots, frame numbers, or acceptance
  results unless they are reproducible by re-running an existing command.
- Do not document commands whose harness was deleted.
- Keep the roadmap MVP-first: post-MVP verification belongs in the deferred
  backlog, not in current milestone acceptance.
- When a plan includes expensive validation, skip that validation. Do not add
  it, and do not block the product change on it.

## Scope gate

Do not introduce a new verification tool, device harness, test matrix,
performance gate, or cross-platform acceptance suite unless the user asks.
Executing tests that already exist does not need a pause.
