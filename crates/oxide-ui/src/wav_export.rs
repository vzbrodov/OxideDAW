//! Render a project snapshot outside the UI and audio threads.
use super::*;
use oxide_audio::export::export_project_as;
use oxide_core::Project;
use std::sync::mpsc::{self, TryRecvError};

type ExportResult = Result<PathBuf, String>;
pub(super) type PendingExport = mpsc::Receiver<ExportResult>;

impl Workspace {
    pub(super) fn suggested_wav_path(&self) -> PathBuf {
        let name = self
            .session
            .path
            .as_deref()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .map(|name| name.strip_suffix(".oxide.json").unwrap_or(name))
            .unwrap_or("beat");
        let parent = self
            .session
            .path
            .as_deref()
            .and_then(Path::parent)
            .unwrap_or(&self.session.media_base);
        parent.join(format!("{name}.wav"))
    }

    pub(super) fn start_wav_export(&mut self, destination: PathBuf, ctx: &egui::Context) {
        if destination.file_name().is_none() {
            self.notice("Choose the path of a new WAV file.", true);
            return;
        }
        let project = self.session.editor.project().clone();
        let paths = std::array::from_fn(|track| self.session.sample_path(track));
        let encoding = self.wav_encoding;
        let (sender, receiver) = mpsc::channel();
        let ctx = ctx.clone();
        match std::thread::Builder::new()
            .name("wav-export".into())
            .spawn(move || {
                let result =
                    write_wav(&project, paths, &destination, encoding).map(|()| destination);
                let _ = sender.send(result);
                ctx.request_repaint();
            }) {
            Ok(_) => {
                self.wav_export = Some(receiver);
                self.file_action = None;
                self.notice("Exporting WAV…", false);
            }
            Err(error) => self.notice(format!("Cannot start WAV export: {error}"), true),
        }
    }

    pub(super) fn poll_wav_export(&mut self) {
        let Some(receiver) = &self.wav_export else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("WAV export stopped unexpectedly".into()),
        };
        self.wav_export = None;
        match result {
            Ok(path) => self.notice(format!("Exported {}", path.display()), false),
            Err(error) => self.notice(error, true),
        }
    }
}

fn write_wav(
    project: &Project,
    paths: [Option<PathBuf>; TRACKS],
    destination: &Path,
    encoding: WavEncoding,
) -> Result<(), String> {
    let mut samples = std::array::from_fn(|_| AudioSample {
        samples: vec![0.0],
        channels: 1,
        sample_rate: project.sample_rate,
    });
    for ((sample, track), path) in samples.iter_mut().zip(&project.tracks).zip(paths) {
        if let Some(path) = path {
            *sample = AudioSample::load(&path).map_err(|error| {
                format!(
                    "Track {} ({}): cannot load {}: {error}",
                    track.id,
                    track.name,
                    path.display()
                )
            })?;
        }
    }
    export_project_as(project, samples, destination, encoding)
        .map_err(|error| format!("Cannot export {}: {error}", destination.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_core::sequencer::ArrangementBar;

    #[test]
    fn background_export_uses_the_selected_format_and_current_edits() {
        let ctx = egui::Context::default();
        let mut workspace = Workspace::new(Session::open(None).unwrap(), &ctx).unwrap();
        workspace
            .session
            .editor
            .apply_batch(&[EditCommand::SetBpm { bpm: 240.0 }])
            .unwrap();
        let destination =
            std::env::temp_dir().join(format!("oxide-float-export-{}.wav", std::process::id()));
        workspace.show_file(FileAction::ExportWav);
        assert!(matches!(workspace.wav_encoding, WavEncoding::Pcm16));
        workspace.wav_encoding = WavEncoding::Float32;
        workspace.start_wav_export(destination.clone(), &ctx);
        let exported = workspace
            .wav_export
            .take()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(exported, destination);
        let bytes = std::fs::read(&destination).unwrap();
        std::fs::remove_file(&destination).unwrap();
        assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 3);
        let frames = u32::from_le_bytes(bytes[46..50].try_into().unwrap());
        assert_eq!(frames, workspace.session.editor.project().sample_rate);
        assert!(workspace.session.dirty());
        assert!(workspace.file_action.is_none());
    }

    #[test]
    fn missing_assigned_media_is_reported_without_publishing_a_wav() {
        let project = Project {
            arrangement: vec![ArrangementBar::Main {}, ArrangementBar::Rest {}],
            ..Project::default()
        };
        // Point into a unique empty directory so neither path can exist.
        let root = std::env::temp_dir().join(format!("oxide-wav-export-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let missing = root.join("missing.wav");
        let destination = root.join("beat.wav");
        let paths = [Some(missing), None, None, None];
        let error = write_wav(&project, paths, &destination, WavEncoding::Pcm24).unwrap_err();
        assert!(error.contains("Track 1") && error.contains("missing.wav"));
        assert!(!destination.exists());
        std::fs::remove_dir(&root).unwrap();
    }
}
