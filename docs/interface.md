# Pattern studio

The native workspace combines a four-channel rhythm panel and step
sequencer, a sample library, an instrument-style sampler panel and stereo output.

![OxideDAW pattern studio](images/workspace.png)

## Visual language

The workspace follows the material vocabulary of studio instruments: a dark
anodised rhythm panel, an aluminium sampler faceplate, moulded silicone pads,
turned metal rotary caps and recessed waveform glass. The original control
renders share a fixed camera and area-light rig. Their reproducible Blender
source is in `crates/oxide-ui/assets/hardware/`; labels, scales, state indicators
and all interactions are native egui drawing rather than a screenshot overlay.

The hardware construction takes reference from
[GarageBand](https://support.apple.com/guide/garageband/welcome/mac) and
[Reason Rack](https://www.reasonstudios.com/rack).
The grouped step rows follow the workflow of
[FL Studio's Channel Rack](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html/channelrack.htm).
The operational hierarchy separates browser, pattern, instrument and output,
as in [Ableton Live](https://www.ableton.com/en/manual/live-concepts/).
No product artwork is bundled.

| Role | Color |
| --- | --- |
| Workspace | `#1B1D1F` |
| Shell panels | `#2A2C2F` |
| Rhythm faceplate | `#444849` → `#2D3133` |
| Sampler aluminium | `#C9C8C0` → `#B6B5AC` |
| Instrument ink | `#2B2D2C` |
| Text | `#E2E3E4` |
| Secondary text | `#A4A7AB` |
| Brass accent | `#D8AB56` |

Track colors are blue, mauve, green and amber. Meter segments use green,
yellow and red for measured signal levels. Typography uses Liberation Sans for
control labels and Liberation Mono for musical position and technical values.
Both are bundled under SIL OFL 1.1, with attribution in
`crates/oxide-ui/assets/fonts/LICENSE`.

## Working areas

**Transport.** Open/save, undo/redo, rewind, stop, play/pause, tempo,
bar/beat/tick position and stereo output meters. File and Edit menus provide
project actions; Library toggles the browser.

**Rhythm panel.** Four channels and sixteen steps, grouped into four beats.
Click a pad to toggle velocity 0/100; right-click to select without changing it.
In the Pattern tab, the ruler above the pads seeks within the current bar. Double-click a channel name to audition its sample.

**Velocity bank.** The sixteen bars show the selected channel's step velocities.
Click or drag a bar to edit; arrow keys make fine changes when it has focus.
The numeric slider also edits the selected step. Clear channel applies all
sixteen edits as one undo action. A velocity drag commits once on release.

**Sampler.** The source display shows the actual envelope, duration, channel
count, sample rate and a time scale. Waveform zoom changes the visible beginning
of the source. Audition level changes the level used on the next preview;
it is a session-only preview setting, multiplied by master output gain.
Drag either knob vertically or use arrow keys when focused. Preview stops
pattern playback; Stop ends preview too. Source path, Load, Unassign and
Audition operate on the selected channel.

**Master.** The sole project output control: a logarithmic fader with a
−60..0 dB scale and separate left/right meters. Peak values come from the audio
callback, accumulate across callback chunks and decay visually. The 0 dB
button sets unity gain.

## Files and editing

Select a channel, then click a WAV, FLAC or MP3 in the library to assign it.
Dropping audio on the window assigns it to the selected channel. The library
supports folder navigation, filtering and the selected channel assignment.

Pattern, tempo and gain edits reach playback through a single validated,
bounded state message. Project slider drags commit on release, preserving
one undo action per gesture. Tempo text commits on Enter or loss of focus.
Assigning a new sample stops pattern playback while the file is decoded.
Unassigned channels are silent; unavailable assigned samples can be replaced
through the library or source-path control.

Open and Save use explicit file paths. New files refuse to overwrite existing
files. Saving elsewhere rebases relative media references while preserving the
meaning of undo snapshots. Closing a changed project opens a save/discard dialog;
opening another project shows an unsaved-changes warning.

Keyboard: Space plays/pauses when no text control is editing; Ctrl+S saves,
Ctrl+Z undoes, Ctrl+Shift+Z redoes, and Ctrl+Q closes. These use Command on macOS.
Smaller windows scroll the workspace; the minimum size is 1040×740.

## Arrangement

Use the **Arrangement** tab above the step sequencer to edit and loop the
saved bar order. **+ Pattern** inserts a bar that follows edits to the main
pattern. **+ Variation** captures the current pattern as an independent bar.
**+ Rest** adds a bar with no new hits.

Select a bar to move, replace or remove it. Double-click a bar to seek while
the transport is active. The highlighted bar follows playback. Switching
tabs or changing the bar order stops playback. Press Play to hear the new
sequence. Main-pattern, tempo and volume edits can still update a running
session. The whole arrangement is saved with the project and each bar edit
is one undo action.

## Run and verification

```sh
cargo run -p oxide-app --features gui -- gui
cargo run -p oxide-app --features gui -- gui /path/to/beat.oxide.json
cargo test --workspace --all-features
cargo clippy --workspace --all-features --all-targets -- -D warnings
```

The UI uses eframe/egui with OpenGL and Linux X11/Wayland support. Building audio
requires ALSA development files. A minimal Debian/Ubuntu X11 environment needs
`libegl1`, `libgl1`, `libxcursor1`, `libxi6`, `libxrandr2`, `libx11-xcb1`, and
`libxkbcommon-x11-0`; regular desktop installations normally provide these.

Native rendering and input are checked under Xvfb with software OpenGL; playback
and meters are checked through ALSA null. Playback on real audio hardware remains
unverified. Piano roll, per-channel mixing, effects and native OS
file-picker dialogs remain future work.
