# Legacy beat release fixtures

OxideDAW contributors authored both documents and generated the sample.
SPDX-License-Identifier: MIT OR Apache-2.0

The mono PCM16 WAV contains 1024 samples of value 8192 at 8000 Hz.
Both projects play this pulse at steps 0 and 15 at 137 BPM. The v2 arrangement
adds a rest, a variation at velocity 64 and a repeat of the main pattern.
Its PCM16 stereo export has 56,206 frames. The five onsets occur at frames
0, 13,139, 28,029, 42,044 and 55,182.

From the workspace root, run the CLI migration and reference-render check and
the desktop session recovery check before a release:

```sh
cargo test --locked -p oxide-app --test project_cli legacy_project_can_save_undo_and_export_an_arrangement
cargo test --locked -p oxide-ui restart_recovers_edits_without_replacing_the_original
```

Both checks work on temporary copies. The recovery scenario deliberately
leaves the WAV behind to exercise a missing relative media reference.
