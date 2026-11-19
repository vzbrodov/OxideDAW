# OxideDAW contributor guide

OxideDAW is an open-source, native DAW written in Rust. It is a clean-room
reimplementation of FL Studio's pattern-based music production workflows.
Use public documentation, public specifications, and independently observed
behavior as references. OxideDAW has its own interface, artwork, and project
format. Describe compatibility by the workflows and formats that have been
tested. The desktop app uses native Rust with egui/eframe. Linux is the first
development target.

## FL Studio compatibility

Support for FL Studio project files (`.flp`) is a project goal. Start with
read-only import into the native OxideDAW project model. Use small, independently
authored fixtures and record which FL Studio versions and project features each
fixture covers. Import supported data faithfully, report unsupported features
and missing media, and leave the source file untouched.

Treat `.flp` export as a separate milestone. Verify exported projects by
reopening them in the FL Studio versions we claim to support. Report any
unsupported or lossy mappings before writing. Document verified coverage for
both directions and keep `.oxide.json` as the native format.

## Start here

| Read | Use it for |
| --- | --- |
| [README.md](README.md) | Build and run commands, CLI examples, and supported workflows |
| [DEVELOPMENT_PLAN.md](DEVELOPMENT_PLAN.md) | Current implementation, MVP scope, and next work |
| [docs/project-format.md](docs/project-format.md) | Project versions, media paths, editing commands, and storage guarantees |
| [docs/interface.md](docs/interface.md) | Desktop controls, interaction design, and runtime requirements |

Check `git status` and recent commits before touching shared files. For issue
work, read the discussion and search open PRs and recent changes for the issue
number and relevant terms. Coordinate on the issue before duplicating active
work. Open the relevant definitions and callers before deciding where to fix a
bug. Keep the change scoped and preserve other contributors' local edits.

## Workspace boundaries

| Crate | Responsibility |
| --- | --- |
| `oxide-core` | Project model, validation, storage, editing history, and musical scheduling |
| `oxide-dsp` | Sample decoding and DSP primitives such as sampler voices |
| `oxide-audio` | Device I/O, playback controls, pattern rendering, mixing, and WAV export |
| `oxide-midi` | MIDI note and clip models |
| `oxide-app` | CLI entry point and optional desktop startup |
| `oxide-ui` | Native desktop views, gestures, and session coordination |
| `oxide-mcp` | Future Model Context Protocol adapter. Currently contains request and response types |
| `oxide-plugin-sdk` | Preliminary plugin API |

Keep GUI, device, and protocol concerns out of the musical model. Put project
mutations in `oxide-core/src/edit.rs` through `EditCommand` and `ProjectEditor`.
Use stable IDs for edits that address tracks. Apply a multi-step gesture or
automation edit as one validated batch and one undo action. Keep playback-only
controls and transient view state out of project history. Send audio controls and
prepared state through the device-facing API in `oxide-audio/src/playback.rs`.

## Audio and musical time

- Keep allocation, heap deallocation, decoding, I/O, logging, and blocking
  operations out of the audio callback. Prepare assets and state on a control
  thread. Keep callback work bounded, including message processing and
  polyphony. Specify what happens when a queue or voice pool is full.
- Reuse the sequencer in `oxide-core/src/sequencer.rs` and the renderer in
  `oxide-audio/src/pattern.rs` for live and offline paths. Schedule events from
  absolute musical positions and round at the frame boundary. Define how
  tempo changes, seek, pause, stop, and loops affect position and sample tails.
- Test timing without an audio device. Cover events on block boundaries,
  fractional tempos, and the final frame when relevant. Test callback behavior
  with the actual device when a change depends on the backend or latency.
  A virtual output confirms routing through the backend.

## Projects and inputs

- Treat projects, media, CLI arguments, and future protocol requests as untrusted.
  Validate sizes, ranges, and finite numeric values before indexing, allocating,
  or rendering. Return actionable errors on invalid input. Keep production code
  safe Rust and avoid panic paths.
- Preserve loading of supported project versions. Change the format with an
  explicit version and migration when compatibility requires it. Test old files
  and save/reopen behavior, including stable IDs and relative media references.
- Use the atomic storage path in `oxide-core/src/storage.rs` when publishing a
  complete file. Preserve the destination on failed writes. Creation and
  replacement are distinct operations. Keep missing media diagnosable and
  rebase relative references when saving to a different directory.

## Clean-room work and assets

- Record the public specification or independent observation behind a
  compatibility change. Implement it independently in Rust and use generated
  or suitably licensed fixtures. Keep proprietary code, decompiled material,
  product artwork, presets, and factory sounds out of the repository.
- Check the license of any new dependency or asset before adding it. Keep asset
  source, author and SPDX license with the asset or in an attribution file.
  Include any notices the license requires. Keep secrets, private projects,
  and personal data out of commits and shared test output.
- Keep the desktop app native. A webview shell or a JavaScript application
  layer does not belong in the desktop build.

## Future MCP control

Design tools around reading project state, applying validated edits, and
controlling transport. Route mutations through the same commands and history
as the desktop and CLI. A tool batch should either apply entirely and undo as
one action, or leave the project intact. Bound payloads and batch sizes, and
return structured errors for invalid arguments, missing media, and rejected
controls. Keep protocol handling off the audio thread. Saving and exporting
should be explicit actions with the same path and overwrite rules as local use.

## Verify and submit

Run the focused tests while iterating. Add a regression test for a bug fix and
test project edits through a real entry point when behavior crosses crates.
For a visual change, open the native app and inspect the affected state.
Before review, run the workspace checks when available.

```sh
cargo fmt --all -- --check
cargo test --locked --workspace --all-features
cargo clippy --locked --workspace --all-features --all-targets -- -D warnings
```

Review the complete diff and report the checks that ran, including any limits on
device or UI verification. Use a topic branch and a pull request for changes to
`main`. Keep build outputs, local notes, and unrelated work out of commits.
Update the roadmap or format documentation when the change affects what users can
do or how projects are stored.
