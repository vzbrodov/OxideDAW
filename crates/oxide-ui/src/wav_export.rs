//! Render a project snapshot outside the UI and audio threads.
use super::*;
use oxide_audio::export::export_project;
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
        let (sender, receiver) = mpsc::channel();
        let ctx = ctx.clone();
        match std::thread::Builder::new()
            .name("wav-export".into())
            .spawn(move || {
                let result = write_wav(&project, paths, &destination).map(|()| destination);
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
    export_project(project, samples, destination)
        .map_err(|error| format!("Cannot export {}: {error}", destination.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_core::sequencer::ArrangementBar;

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
        let error = write_wav(&project, paths, &destination).unwrap_err();
        assert!(error.contains("Track 1") && error.contains("missing.wav"));
        assert!(!destination.exists());
        std::fs::remove_dir(&root).unwrap();
    }
}
