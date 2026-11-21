use oxide_core::{
    edit::{EditCommand, ProjectEditor},
    project::ProjectResult,
    sequencer::Pattern,
    Project,
};
use std::path::{Path, PathBuf};

pub(crate) struct Session {
    pub editor: ProjectEditor,
    pub path: Option<PathBuf>,
    pub media_base: PathBuf,
    saved: Option<Project>,
}

impl Session {
    pub fn open(path: Option<&Path>) -> ProjectResult<Self> {
        let (project, path) = if let Some(path) = path {
            let path = std::fs::canonicalize(path)?;
            (Project::load(&path)?, Some(path))
        } else {
            let mut project = Project {
                name: "Untitled beat".into(),
                bpm: 120.0,
                pattern: Pattern::drum_demo(),
                ..Project::default()
            };
            for (track, name) in
                project
                    .tracks
                    .iter_mut()
                    .zip(["Kick", "Snare", "Hi-hat", "Percussion"])
            {
                track.name = name.into();
            }
            (project, None)
        };
        let media_base = path
            .as_ref()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf)
            .unwrap_or(std::env::current_dir()?);
        let saved = path.as_ref().map(|_| project.clone());
        Ok(Self {
            editor: ProjectEditor::new(project)?,
            path,
            media_base,
            saved,
        })
    }

    pub fn dirty(&self) -> bool {
        self.saved.as_ref() != Some(self.editor.project())
    }

    pub fn recover(path: &Path) -> ProjectResult<Self> {
        let mut session = Self::open(Some(path))?;
        session.media_base = std::env::current_dir()?;
        session.path = None;
        session.saved = None;
        Ok(session)
    }

    pub fn sample_path(&self, track: usize) -> Option<PathBuf> {
        self.editor.project().tracks[track]
            .sample
            .as_ref()
            .map(|p| self.media_base.join(p))
    }

    pub fn assign(&mut self, track: usize, path: &Path) -> ProjectResult<bool> {
        let path = std::fs::canonicalize(path)?;
        self.editor.apply_batch(&[EditCommand::SetSample {
            track_id: self.editor.project().tracks[track].id,
            sample: Some(path),
        }])
    }

    /// Rebase a disk copy, keeping the edit history anchored to its original media base.
    /// Saving elsewhere never changes the meaning of old undo snapshots.
    pub fn save(&mut self, target: &Path) -> ProjectResult<()> {
        let target = if target.is_absolute() {
            target.to_owned()
        } else {
            std::env::current_dir()?.join(target)
        };
        let parent = target
            .parent()
            .ok_or("Expected a project directory")?
            .canonicalize()?;
        let target = parent.join(target.file_name().ok_or("Expected a project file name")?);
        let document = self.document_for(Some(&parent));
        if self.path.as_ref() == Some(&target) {
            document.save(&target)?;
        } else {
            document.save_new(&target)?;
        }
        self.path = Some(target);
        self.saved = Some(self.editor.project().clone());
        Ok(())
    }

    pub fn document_for(&self, parent: Option<&Path>) -> Project {
        let mut document = self.editor.project().clone();
        for track in &mut document.tracks {
            if let Some(sample) = &track.sample {
                let absolute = self.media_base.join(sample);
                track.sample = Some(
                    parent
                        .and_then(|parent| absolute.strip_prefix(parent).ok())
                        .unwrap_or(&absolute)
                        .to_owned(),
                );
            }
        }
        document
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn save_as_rebases_missing_media_preserves_history_and_refuses_overwrite() {
        let root = std::env::temp_dir().join(format!("oxide-ui-save-{}", std::process::id()));
        std::fs::create_dir_all(root.join("original")).unwrap();
        std::fs::create_dir_all(root.join("copy")).unwrap();
        let original = root.join("original/beat.oxide.json");
        let mut project = Project::default();
        project.tracks[0].sample = Some("missing.wav".into());
        project.save_new(&original).unwrap();
        let mut session = Session::open(Some(&original)).unwrap();
        session
            .editor
            .apply_batch(&[EditCommand::SetBpm { bpm: 123.0 }])
            .unwrap();
        let target = root.join("copy/beat.oxide.json");
        session.save(&target).unwrap();
        assert!(!session.dirty());
        let loaded = Project::load(&target).unwrap();
        assert_eq!(
            loaded.sample_path(&target, 0).unwrap(),
            root.join("original/missing.wav")
        );
        assert!(session.editor.undo());
        assert!(session.dirty());
        assert_eq!(
            session.sample_path(0).unwrap(),
            root.join("original/missing.wav")
        );
        assert!(session.save(&original).is_err());
        session.save(&target).unwrap();
        assert_eq!(Project::load(&target).unwrap().bpm, project.bpm);
        std::fs::remove_dir_all(root).unwrap();
    }
}
