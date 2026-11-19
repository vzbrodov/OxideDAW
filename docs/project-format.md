# Project format v2

OxideDAW stores a four-track session as UTF-8 JSON. The suggested
extension is `.oxide.json`. Media is referenced separately and never embedded.

| Field | Meaning |
| --- | --- |
| `format_version` | Required integer `2`. Loading v1 upgrades the project in memory. An explicit save writes v2. |
| `name` | Nonempty project name. |
| `bpm` | Finite tempo between 1 and 999. |
| `sample_rate` | Positive target offline sample rate; must allow at least one frame per step. Live output follows the device's sample rate. |
| `master_volume` | Finite gain from 0 to 1. |
| `tracks` | Exactly four entries, in the same order as the pattern rows. |
| `tracks[].id` | Nonzero unique integer identity, retained across saves. |
| `tracks[].name` | Nonempty display name. |
| `tracks[].sample` | UTF-8 path, or `null` when unassigned. |
| `pattern.steps` | Exactly four arrays of sixteen integer velocities. `0` disables the step; `1..127` triggers a sample. |
| `arrangement` | Optional list of bars, in playback order. An omitted or empty list exports the main pattern once. |

Unknown fields, invalid array sizes, malformed JSON and out-of-range values are
rejected. The earlier three-field `Project` Rust scaffold did not define an
on-disk format; no migration from that scaffold is provided.

## Media paths

Paths inside the project directory are stored relative to that directory by
`project-new`. External media uses canonical absolute paths. Media is not copied.
For a portable session, put the WAVs next to the JSON or in a subdirectory, then
move that directory as a unit. Manually written relative paths may also use `..`.

A missing or unassigned sample does not stop `Project::load` or `project-info`.
Pattern mode requires all four samples. Arrangement playback and export treat
unassigned tracks as silent. Assigned files must decode successfully, with
errors identifying the track and path. Decoding finishes before playback.

## Arrangement and export

Every bar covers 16 steps and uses the project BPM, tracks and master volume.
Each entry has one of these shapes.

| Entry | Meaning |
| --- | --- |
| `{"type":"main"}` | Use the main pattern stored in the project. Edits to the main pattern apply to every such bar. |
| `{"type":"variation","pattern":{...}}` | Use an independent pattern with the same `steps` shape and velocity range as the main pattern. |
| `{"type":"rest"}` | Advance one bar without starting new hits. Already sounding samples can ring through it. |

`project-export` renders all bars in order and drains any remaining sample
tails at the end. `project-play` loops the main pattern by default.
`project-play PROJECT --arrangement [DEVICE]` loops the complete arrangement
at the audio device's sample rate. Voice tails can overlap the wrap into
the first bar. In this mode, `seek` counts steps across bars starting at zero.
It clears the current voices and preserves play/pause.

## Editing

Close playback, edit with `project-edit`, save, and reopen with `project-play`.
The editor accepts a JSON command or array per line; arrays are atomic batches.
`undo`/`redo` restore a whole batch; `show` prints the current document, `save`
atomically saves it, and `quit` or EOF discards unsaved changes. History is
session-local and capped at 128 actions.

| Command | Fields |
| --- | --- |
| `set_step` | `track_id`, zero-based `step` (0..15), `velocity` (0..127) |
| `set_bpm` | `bpm` (1..999) |
| `set_master_volume` | `volume` (0..1) |
| `set_sample` | `track_id`, `sample` (stored path or `null` to unassign) |
| `set_arrangement` | `bars` (the complete ordered list, or `[]` for single-pattern export) |

For example: `{"command":"set_step","track_id":1,"step":0,"velocity":100}`.
Sample references use the same rules as the document; editing does not copy,
decode or require media. Unknown command fields and IDs are rejected.
Each command must be valid, including intermediate states within a batch.
Failed and no-op batches preserve undo/redo history. A new successful edit clears
redo. The shared Rust API is `oxide_core::edit::{EditCommand, ProjectEditor}`;
it owns the document and exposes read-only state, keeping changes in history.
All editing, snapshot allocation and storage happen outside the audio callback.

Direct JSON editing is also supported: change `pattern.steps[track][step]` to set
a velocity; rows and steps are zero-indexed.
Track IDs identify the existing slots and should remain stable when renaming.
A pattern row belongs to its track slot, so moving a track requires moving its
row as well. The main pattern and arrangement variations share those slots.

## Storage guarantees

`Project::save` validates and serializes before writing. It creates a sibling
temporary file exclusively, writes and syncs it, then atomically renames it over
the target. Validation, write or rename failures preserve the previous document.
The temporary file is removed on ordinary error paths. On Unix, the directory is
also synced after publication. A process or machine crash may leave a temporary
file, which is not loaded as the project automatically.

`Project::save_new`, used by `project-new`, publishes through a sibling hard link
and fails if the destination exists, including when another process creates it
concurrently. The project directory's filesystem must support hard links.

Saving under a different directory retains relative path spellings. A future
Save As operation must rebase references or copy media explicitly. Media files
are not covered by the project's atomic save. Directory-sync failures occur
after publication and can report an error even though the new document exists.
