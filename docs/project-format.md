# Project format v1

OxideDAW stores a four-track, 16-step session as UTF-8 JSON. The suggested
extension is `.oxide.json`. Media is referenced separately and never embedded.

| Field | Meaning |
| --- | --- |
| `format_version` | Required integer `1`. Unsupported versions are rejected. |
| `name` | Nonempty project name. |
| `bpm` | Finite tempo between 1 and 999. |
| `sample_rate` | Positive target offline sample rate; must allow at least one frame per step. Live output follows the device's sample rate. |
| `master_volume` | Finite gain from 0 to 1. |
| `tracks` | Exactly four entries, in the same order as the pattern rows. |
| `tracks[].id` | Nonzero unique integer identity, retained across saves. |
| `tracks[].name` | Nonempty display name. |
| `tracks[].sample` | UTF-8 path, or `null` when unassigned. |
| `pattern.steps` | Exactly four arrays of sixteen integer velocities. `0` disables the step; `1..127` triggers a sample. |

Unknown fields, invalid array sizes, malformed JSON and out-of-range values are
rejected. The earlier three-field `Project` Rust scaffold did not define an
on-disk format; no migration from that scaffold is provided.

## Media paths

Paths inside the project directory are stored relative to that directory by
`project-new`. External media uses canonical absolute paths. Media is not copied.
For a portable session, put the WAVs next to the JSON or in a subdirectory, then
move that directory as a unit. Manually written relative paths may also use `..`.

A missing or unassigned sample does not stop `Project::load` or `project-info`.
`project-play` requires all four samples and reports a track ID, name and file
path when decoding fails. All decoding happens before starting audio playback.

## Editing

For now, close playback, edit the JSON, and reopen with `project-play`. Change
`pattern.steps[track][step]` to set a velocity; rows and steps are zero-indexed.
Track IDs identify the existing slots and should remain stable when renaming.
A pattern row belongs to its track slot, so moving a track requires moving its
row as well. There is one looping pattern; clips, arrangement, automation,
editing commands and undo/redo remain planned.

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
