//! Native pattern workspace. File I/O, decoding and edits stay on the UI thread.
mod arranger;
mod instrument;
mod recovery;
mod session;
mod skin;
mod theme;
mod views;
mod wav_export;
mod widgets;

use eframe::egui::{self, pos2, vec2, Align2, FontId, Rect, RichText, Sense, Stroke, StrokeKind};
use oxide_audio::export::WavEncoding;
use oxide_audio::playback::{Playback, PlaybackCommand};
use oxide_core::{
    edit::EditCommand,
    project::ProjectResult,
    sequencer::{ArrangementBar, Pattern, PlaybackBar, STEPS, TRACKS},
};
use oxide_dsp::sample::AudioSample;
use recovery::Recovery;
use session::Session;
use std::{
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::Duration,
};
use theme::*;

pub fn run(path: Option<&Path>) -> ProjectResult<()> {
    let session = Session::open(path)?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 940.0])
            .with_min_inner_size([1040.0, 740.0]),
        ..Default::default()
    };
    eframe::run_native(
        "OxideDAW",
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            let mut workspace = Workspace::new(session, &cc.egui_ctx)?;
            match Recovery::start(&workspace.session) {
                Ok(recovery) => workspace.recovery = Some(recovery),
                Err(error) => workspace.notice(format!("Recovery is unavailable: {error}"), true),
            }
            Ok(Box::new(workspace))
        }),
    )
    .map_err(|error| error.to_string().into())
}

#[derive(Clone, Copy)]
enum FileAction {
    Open,
    Save,
    SaveAndClose,
    ExportWav,
}

struct Workspace {
    session: Session,
    recovery: Option<Recovery>,
    skin: skin::Skin,
    preview_volume: f32,
    samples: [Option<AudioSample>; TRACKS],
    loaded_paths: [Option<PathBuf>; TRACKS],
    media_errors: [Option<String>; TRACKS],
    peaks: [Vec<f32>; TRACKS],
    playback: Option<Playback>,
    playing: bool,
    arrangement_mode: bool,
    selected_bar: usize,
    playback_bars: Vec<PlaybackBar>,
    selected_pattern: Option<u64>,
    new_pattern: Option<instrument::NewPattern>,
    selected: usize,
    selected_step: usize,
    bpm: f64,
    volume: f32,
    velocity: u8,
    folder: String,
    filter: String,
    files: Vec<PathBuf>,
    status: String,
    error: bool,
    file_action: Option<FileAction>,
    file_input: String,
    wav_encoding: WavEncoding,
    wav_export: Option<wav_export::PendingExport>,
    confirm_close: bool,
    allow_close: bool,
    save_requested: bool,
    levels: [f32; TRACKS + 2],
    preview: Option<Playback>,
    preview_until: Option<std::time::Instant>,
    sample_input: String,
    wave_zoom: f32,
    browser_visible: bool,
}

impl Workspace {
    fn new(session: Session, ctx: &egui::Context) -> Result<Self, image::ImageError> {
        let bpm = session.editor.project().bpm;
        let volume = session.editor.project().master_volume;
        let folder = session.media_base.display().to_string();
        let velocity = session.editor.project().pattern.get(0, 0).unwrap_or(0);
        let mut app = Self {
            session,
            recovery: None,
            skin: skin::Skin::new(ctx)?,
            preview_volume: 0.7,
            samples: std::array::from_fn(|_| None),
            loaded_paths: std::array::from_fn(|_| None),
            media_errors: std::array::from_fn(|_| None),
            peaks: std::array::from_fn(|_| Vec::new()),
            playback: None,
            playing: false,
            arrangement_mode: false,
            selected_bar: 0,
            playback_bars: Vec::new(),
            selected_pattern: None,
            new_pattern: None,
            selected: 0,
            selected_step: 0,
            bpm,
            volume,
            velocity,
            folder,
            filter: String::new(),
            files: Vec::new(),
            status: "Choose a sample in the browser to assign it to the selected channel.".into(),
            error: false,
            file_action: None,
            file_input: String::new(),
            wav_encoding: WavEncoding::default(),
            wav_export: None,
            confirm_close: false,
            allow_close: false,
            save_requested: false,
            levels: [0.0; TRACKS + 2],
            preview: None,
            preview_until: None,
            sample_input: String::new(),
            wave_zoom: 1.0,
            browser_visible: true,
        };
        app.reload_samples();
        if app.media_errors.iter().any(Option::is_some) {
            app.notice("Some samples are unavailable. Select a channel and choose a replacement in the browser.", true);
        } else if app.samples.iter().any(Option::is_some) {
            app.notice(
                "Ready. Click a step to edit the beat; press Play to listen.",
                false,
            );
        }
        app.sample_input = app
            .session
            .sample_path(0)
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        app.refresh_browser();
        Ok(app)
    }

    fn notice(&mut self, message: impl Into<String>, error: bool) {
        self.status = message.into();
        self.error = error;
    }

    fn refresh_browser(&mut self) {
        match std::fs::read_dir(&self.folder) {
            Ok(entries) => {
                self.files = entries
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.is_dir() || is_audio(p))
                    .take(2000)
                    .collect();
                self.files
                    .sort_by_key(|p| (!p.is_dir(), p.file_name().map(|s| s.to_ascii_lowercase())));
            }
            Err(error) => self.notice(format!("Cannot open folder: {error}"), true),
        }
    }

    fn reload_samples(&mut self) {
        for track in 0..TRACKS {
            let path = self.session.sample_path(track);
            if self.loaded_paths[track] == path {
                continue;
            }
            self.playback = None;
            self.playing = false;
            self.preview = None;
            self.preview_until = None;
            self.samples[track] = None;
            self.peaks[track].clear();
            self.media_errors[track] = None;
            if let Some(path) = &path {
                match AudioSample::load(path) {
                    Ok(sample) => {
                        let frames = sample.samples.len() / sample.channels;
                        let chunk = (frames / 320).max(1) * sample.channels;
                        self.peaks[track] = sample
                            .samples
                            .chunks(chunk)
                            .map(|c| c.iter().fold(0.0_f32, |peak, v| peak.max(v.abs())))
                            .collect();
                        self.samples[track] = Some(sample);
                    }
                    Err(error) => {
                        self.media_errors[track] = Some(format!("{}: {error}", path.display()))
                    }
                }
            }
            self.loaded_paths[track] = path;
        }
    }

    fn changed(&mut self) {
        self.bpm = self.session.editor.project().bpm;
        self.volume = self.session.editor.project().master_volume;
        if self.selected_pattern.is_some() && self.named_pattern().is_none() {
            self.select_pattern(None);
        }
        self.velocity = self
            .rack_pattern()
            .get(self.selected, self.selected_step)
            .unwrap_or(0);
        self.reload_samples();
        self.sample_input = self
            .session
            .sample_path(self.selected)
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let project = self.session.editor.project();
        if self.arrangement_mode
            && self.playback.is_some()
            && project.playback_bars().as_ref().ok() != Some(&self.playback_bars)
        {
            self.playback = None;
            self.playing = false;
            self.notice(
                "Arrangement changed. Press Play to hear the updated arrangement.",
                false,
            );
            return;
        }
        self.send(PlaybackCommand::SetState {
            pattern: self.transport_pattern().clone(),
            bpm: project.bpm,
            volume: project.master_volume,
        });
    }

    fn edit(&mut self, commands: &[EditCommand]) -> bool {
        match self.session.editor.apply_batch(commands) {
            Ok(true) => {
                self.changed();
                true
            }
            Ok(false) => false,
            Err(error) => {
                self.notice(error.to_string(), true);
                false
            }
        }
    }

    fn send(&mut self, command: PlaybackCommand) {
        if let Some(playback) = &mut self.playback {
            if let Err(error) = playback.command(command) {
                self.notice(
                    format!("Playback stopped: {error}. Press Play to restart."),
                    true,
                );
                self.playback = None;
                self.playing = false;
            }
        }
    }

    fn toggle_play(&mut self) {
        self.preview = None;
        self.preview_until = None;
        let playing_notice = if self.arrangement_mode {
            "Playing arrangement. Space pauses; stop returns to the first bar."
        } else {
            "Playing selected pattern. Space pauses; stop returns to the first step."
        };
        if self.playing {
            self.send(PlaybackCommand::Pause);
            self.playing = false;
            if self.playback.is_some() {
                self.notice("Paused. Press Play to resume from this position.", false);
            }
            return;
        }
        if self.playback.is_some() {
            self.send(PlaybackCommand::Play);
            self.playing = self.playback.is_some();
            if self.playing {
                self.notice(playing_notice, false);
            }
            return;
        }
        if let Some(error) = self.media_errors.iter().flatten().next() {
            self.notice(
                format!("Relink the missing or unreadable sample: {error}"),
                true,
            );
            return;
        }
        if self.samples.iter().all(Option::is_none) {
            self.notice(
                "Select a channel, then choose an audio file in the browser.",
                true,
            );
            return;
        }
        let project = self.session.editor.project();
        let samples = std::array::from_fn(|i| {
            self.samples[i].clone().unwrap_or_else(|| AudioSample {
                samples: vec![0.0],
                channels: 1,
                sample_rate: project.sample_rate,
            })
        });
        let bars = if self.arrangement_mode {
            match project.playback_bars() {
                Ok(bars) => bars,
                Err(error) => {
                    self.notice(format!("Cannot prepare arrangement: {error}"), true);
                    return;
                }
            }
        } else {
            Vec::new()
        };
        match Playback::start_arrangement(
            samples,
            self.transport_pattern().clone(),
            bars.clone(),
            project.bpm,
            None,
            project.master_volume,
        ) {
            Ok(playback) => {
                self.playback = Some(playback);
                self.playback_bars = bars;
                self.playing = true;
                self.notice(playing_notice, false);
            }
            Err(error) => self.notice(format!("Cannot start audio: {error}"), true),
        }
    }

    fn stop(&mut self) {
        self.preview = None;
        self.preview_until = None;
        let had_stream = self.playback.is_some();
        self.send(PlaybackCommand::Stop);
        self.playing = false;
        if self.playback.is_some() || !had_stream {
            self.notice("Stopped. Press Play to start from the first step.", false);
        }
    }

    fn assign(&mut self, path: &Path) {
        match AudioSample::load(path) {
            Ok(_) => match self.session.assign(self.selected, path) {
                Ok(_) => {
                    self.changed();
                    self.notice(
                        format!(
                            "Assigned {}. Press Play to hear the pattern.",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ),
                        false,
                    );
                }
                Err(error) => self.notice(error.to_string(), true),
            },
            Err(error) => self.notice(format!("Cannot load {}: {error}", path.display()), true),
        }
    }

    fn save(&mut self, ctx: &egui::Context, close_after: bool) {
        if let Some(path) = self.session.path.clone() {
            self.save_to(ctx, &path, close_after);
        } else if close_after {
            self.show_file(FileAction::SaveAndClose);
        } else {
            self.show_file(FileAction::Save);
        }
    }

    fn save_to(&mut self, ctx: &egui::Context, path: &Path, close_after: bool) {
        match self.session.save(path) {
            Ok(()) => {
                self.file_action = None;
                self.notice(format!("Saved {}", path.display()), false);
                self.clear_recovery();
                if close_after {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Err(error) => self.notice(format!("Cannot save: {error}"), true),
        }
    }

    fn clear_recovery(&mut self) {
        if let Some(recovery) = self.recovery.as_mut() {
            if let Err(error) = recovery.clear(&self.session) {
                self.notice(format!("Cannot remove recovery snapshot: {error}"), true);
            }
        }
    }

    fn recovery_dialog(&mut self, ctx: &egui::Context) {
        let Some(recovery) = self.recovery.as_mut() else {
            return;
        };
        let Some((_, session)) = recovery.candidate.as_ref() else {
            return;
        };
        let mut restore = false;
        let mut skip = false;
        egui::Modal::new(egui::Id::new("project-recovery")).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.heading("Recover unsaved edits?");
            match session {
                Ok(session) => {
                    ui.label(&session.editor.project().name);
                    ui.label("Recover into a new project file.");
                }
                Err(error) => {
                    ui.label(RichText::new(format!("Cannot read snapshot: {error}")).color(COPPER));
                }
            }
            ui.horizontal(|ui| {
                skip = ui.button("Later").clicked();
                restore = ui
                    .add_enabled(session.is_ok(), egui::Button::new("Recover project"))
                    .clicked();
            });
        });
        if skip {
            recovery.candidate = None;
        } else if let Some((candidate, Ok(session))) =
            restore.then(|| recovery.candidate.take()).flatten()
        {
            match Self::new(session, ctx) {
                Ok(mut workspace) => {
                    if let Err(error) = recovery.accept(candidate, &workspace.session) {
                        workspace.notice(format!("Cannot adopt recovery snapshot: {error}"), true);
                    }
                    workspace.recovery = self.recovery.take();
                    *self = workspace;
                }
                Err(error) => {
                    self.notice(format!("Cannot recover project: {error}"), true);
                }
            }
        }
    }

    fn show_file(&mut self, action: FileAction) {
        self.file_action = Some(action);
        if matches!(action, FileAction::ExportWav) {
            self.file_input = self.suggested_wav_path().display().to_string();
            self.wav_encoding = WavEncoding::default();
            return;
        }
        self.file_input = self
            .session
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| {
                self.session
                    .media_base
                    .join("beat.oxide.json")
                    .display()
                    .to_string()
            });
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.file_action.is_some()
            || self.new_pattern.is_some()
            || self.confirm_close
            || self
                .recovery
                .as_ref()
                .is_some_and(|recovery| recovery.candidate.is_some())
        {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Q)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let save = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S));
        if save {
            self.save_requested = true;
        }
        let redo = ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::Z,
            )
        });
        let undo =
            !redo && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
        if undo && self.session.editor.undo() {
            self.changed();
        }
        if redo && self.session.editor.redo() {
            self.changed();
        }
        if !ctx.wants_keyboard_input()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space))
        {
            self.toggle_play();
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(action) = self.file_action {
            let title = match action {
                FileAction::Open => "Open project",
                FileAction::Save => "Save project",
                FileAction::SaveAndClose => "Save and close",
                FileAction::ExportWav => "Export WAV",
            };
            egui::Modal::new(egui::Id::new("project-file")).show(ctx, |ui| {
                ui.set_width(480.0);
                ui.heading(title);
                if matches!(action, FileAction::Open) && self.session.dirty() {
                    ui.label(RichText::new("Opening another project will discard unsaved edits. Cancel and save first to keep them.").color(COPPER));
                }
                ui.label(if matches!(action, FileAction::ExportWav) {
                    "Output file (.wav)"
                } else {
                    "Project file (.oxide.json)"
                });
                ui.add(egui::TextEdit::singleline(&mut self.file_input).desired_width(f32::INFINITY));
                if matches!(action, FileAction::ExportWav) {
                    ui.add_space(8.0);
                    ui.label("WAV format");
                    ui.horizontal_wrapped(|ui| {
                        ui.radio_value(&mut self.wav_encoding, WavEncoding::Pcm16, "16-bit PCM");
                        ui.radio_value(&mut self.wav_encoding, WavEncoding::Pcm24, "24-bit PCM");
                        ui.radio_value(&mut self.wav_encoding, WavEncoding::Float32, "32-bit float");
                    });
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() { self.file_action = None; }
                    if ui.button(title).clicked() {
                        let path = PathBuf::from(self.file_input.trim());
                        match action {
                            FileAction::Open => match Session::open(Some(&path)).and_then(|session| Self::new(session, ctx).map_err(Into::into)) {
                                Ok(mut workspace) => {
                                    workspace.wav_export = self.wav_export.take();
                                    workspace.recovery = self.recovery.take();
                                    workspace.clear_recovery();
                                    *self = workspace;
                                }
                                Err(error) => self.notice(format!("Cannot open project: {error}"), true),
                            },
                            FileAction::ExportWav => self.start_wav_export(path, ctx),
                            FileAction::Save | FileAction::SaveAndClose => {
                                self.save_to(ctx, &path, matches!(action, FileAction::SaveAndClose));
                            }
                        }
                    }
                });
                if self.error { ui.label(RichText::new(&self.status).color(COPPER)); }
            });
        }
        if self.confirm_close {
            egui::Modal::new(egui::Id::new("unsaved-close")).show(ctx, |ui| {
                ui.set_width(360.0);
                ui.heading("Keep this beat?");
                ui.label("There are unsaved edits in this project.");
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.confirm_close = false;
                    }
                    if ui.button("Discard and close").clicked() {
                        self.allow_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.button("Save and close").clicked() {
                        self.confirm_close = false;
                        self.save(ctx, true);
                    }
                });
            });
        }
    }
}

impl eframe::App for Workspace {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_wav_export();
        if ctx.input(|i| i.viewport().close_requested()) {
            if let Some(recovery) = self.recovery.as_mut() {
                recovery.candidate = None;
            }
            if self.wav_export.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.notice("Wait for WAV export to finish before closing.", false);
            } else if self.session.dirty() && !self.allow_close {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.new_pattern = None;
                self.confirm_close = true;
            } else if let Some(recovery) = self.recovery.as_ref() {
                if let Err(error) = recovery.finish() {
                    eprintln!("Cannot remove recovery snapshot: {error}");
                }
            }
        }
        if self
            .playback
            .as_ref()
            .is_some_and(|p| p.failed.load(Ordering::Relaxed))
        {
            self.playback = None;
            self.playing = false;
            self.notice(
                "Audio device disconnected. Check the output device, then press Play.",
                true,
            );
        }
        if self
            .preview_until
            .is_some_and(|deadline| std::time::Instant::now() >= deadline)
        {
            self.preview = None;
            self.preview_until = None;
            self.notice("Audition finished. Press Play to hear the pattern.", false);
        }
        let mut peaks = self
            .playback
            .as_ref()
            .map_or([0.0; TRACKS + 2], Playback::take_peaks);
        if let Some(preview) = &self.preview {
            for (value, peak) in peaks.iter_mut().zip(preview.take_peaks()) {
                *value = value.max(peak);
            }
        }
        for (level, peak) in self.levels.iter_mut().zip(peaks) {
            *level = (*level * 0.88).max(peak);
        }
        self.shortcuts(ctx);
        let recovery_pending = self
            .recovery
            .as_ref()
            .is_some_and(|recovery| recovery.candidate.is_some());
        if let Some(path) = ctx
            .input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone()))
            .filter(|_| !recovery_pending)
        {
            self.assign(&path);
        }
        self.transport(ctx);
        self.status_bar(ctx);
        if self.browser_visible {
            self.browser(ctx);
        }
        self.master(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BACK).inner_margin(14))
            .show(ctx, |ui| {
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_width(650.0);
                        self.sequence_view(ui);
                        self.rack(ui);
                        self.inspector(ui);
                    });
            });
        if self.save_requested {
            self.save_requested = false;
            self.save(ctx, false);
        }
        self.dialogs(ctx);
        self.new_pattern_dialog(ctx);
        self.recovery_dialog(ctx);
        if !ctx.input(|i| i.viewport().close_requested()) {
            if let Some(recovery) = self.recovery.as_mut() {
                if let Err(error) = recovery.poll(&self.session, ctx) {
                    self.notice(format!("Cannot write recovery snapshot: {error}"), true);
                }
            }
        }
        if self.playing || self.preview.is_some() || self.levels.iter().any(|v| *v > 0.001) {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }
}

fn is_audio(path: &Path) -> bool {
    path.extension().is_some_and(|ext| {
        ["wav", "flac", "mp3"]
            .iter()
            .any(|s| ext.eq_ignore_ascii_case(s))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_close_requires_a_successful_write() {
        let ctx = egui::Context::default();
        let mut workspace = Workspace::new(Session::open(None).unwrap(), &ctx).unwrap();
        let path =
            std::env::temp_dir().join(format!("oxide-close-{}.oxide.json", std::process::id()));
        oxide_core::Project::default().save_new(&path).unwrap();

        let output = ctx.run(egui::RawInput::default(), |ctx| {
            workspace.save(ctx, true);
            assert!(matches!(
                workspace.file_action,
                Some(FileAction::SaveAndClose)
            ));
            workspace.save_to(ctx, &path, true);
        });
        assert!(workspace.error);
        assert!(workspace.session.dirty());
        assert!(!output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Close));

        std::fs::remove_file(&path).unwrap();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            workspace.save_to(ctx, &path, true);
        });
        assert!(!workspace.session.dirty());
        assert!(output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Close));
        assert_eq!(oxide_core::Project::load(&path).unwrap().bpm, 120.0);

        workspace
            .session
            .editor
            .apply_batch(&[EditCommand::SetBpm { bpm: 130.0 }])
            .unwrap();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            workspace.save(ctx, true);
        });
        assert!(output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Close));
        assert_eq!(oxide_core::Project::load(&path).unwrap().bpm, 130.0);
        std::fs::remove_file(&path).unwrap();
    }
}
