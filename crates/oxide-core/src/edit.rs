//! Shared control-thread editing API. One successful batch is one undo action.
use crate::{
    project::{NamedPattern, ProjectResult},
    sequencer::{ArrangementBar, Pattern},
    Project,
};
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, path::PathBuf};

/// Track selectors use stable IDs; step indices are zero-based.
/// Sample paths are stored references, relative to the project directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditCommand {
    SetStep {
        track_id: u64,
        step: usize,
        velocity: u8,
    },
    SetBpm {
        bpm: f64,
    },
    SetMasterVolume {
        volume: f32,
    },
    SetSample {
        track_id: u64,
        sample: Option<PathBuf>,
    },
    SetArrangement {
        bars: Vec<ArrangementBar>,
    },
    InsertBar {
        index: usize,
        bar: ArrangementBar,
    },
    AddPattern {
        pattern_id: u64,
        name: String,
        #[serde(default)]
        pattern: Pattern,
    },
    SetPatternStep {
        pattern_id: u64,
        track_id: u64,
        step: usize,
        velocity: u8,
    },
    RenamePattern {
        pattern_id: u64,
        name: String,
    },
    RemovePattern {
        pattern_id: u64,
    },
}

/// Owns the editable document so callers cannot bypass validation or history.
/// History retains snapshots for at most 128 undo actions.
/// History is session-local and is never serialized into project documents.
pub struct ProjectEditor {
    project: Project,
    undo: VecDeque<Project>,
    redo: Vec<Project>,
}

impl ProjectEditor {
    pub const HISTORY_LIMIT: usize = 128;

    pub fn new(project: Project) -> ProjectResult<Self> {
        project.validate()?;
        Ok(Self {
            project,
            undo: VecDeque::new(),
            redo: Vec::new(),
        })
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Validate each command on a private candidate before publishing anything.
    /// Errors and no-op batches leave both history branches unchanged.
    pub fn apply_batch(&mut self, commands: &[EditCommand]) -> ProjectResult<bool> {
        let mut candidate = self.project.clone();
        for command in commands {
            match command {
                EditCommand::SetStep {
                    track_id,
                    step,
                    velocity,
                } => {
                    let track = track_index(&candidate, *track_id)?;
                    candidate.pattern.set(track, *step, *velocity)?;
                }
                EditCommand::SetBpm { bpm } => candidate.bpm = *bpm,
                EditCommand::SetMasterVolume { volume } => candidate.master_volume = *volume,
                EditCommand::SetSample { track_id, sample } => {
                    let track = track_index(&candidate, *track_id)?;
                    candidate.tracks[track].sample = sample.clone();
                }
                EditCommand::SetArrangement { bars } => candidate.arrangement.clone_from(bars),
                EditCommand::InsertBar { index, bar } => {
                    let count = candidate.arrangement.len();
                    if *index > count {
                        return Err(format!(
                            "Bar insertion index {index} exceeds bar count {count}"
                        )
                        .into());
                    }
                    candidate.arrangement.insert(*index, bar.clone());
                }
                EditCommand::AddPattern {
                    pattern_id,
                    name,
                    pattern,
                } => {
                    candidate.patterns.push(NamedPattern {
                        id: *pattern_id,
                        name: name.clone(),
                        pattern: pattern.clone(),
                    });
                }
                EditCommand::SetPatternStep {
                    pattern_id,
                    track_id,
                    step,
                    velocity,
                } => {
                    let track = track_index(&candidate, *track_id)?;
                    let index = pattern_index(&candidate, *pattern_id)?;
                    candidate.patterns[index]
                        .pattern
                        .set(track, *step, *velocity)?;
                }
                EditCommand::RenamePattern { pattern_id, name } => {
                    let index = pattern_index(&candidate, *pattern_id)?;
                    candidate.patterns[index].name.clone_from(name);
                }
                EditCommand::RemovePattern { pattern_id } => {
                    let index = pattern_index(&candidate, *pattern_id)?;
                    candidate.patterns.remove(index);
                }
            }
            candidate.validate()?;
        }
        if candidate == self.project {
            return Ok(false);
        }
        self.undo
            .push_back(std::mem::replace(&mut self.project, candidate));
        if self.undo.len() > Self::HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.redo.clear();
        Ok(true)
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop_back() else {
            return false;
        };
        self.redo
            .push(std::mem::replace(&mut self.project, previous));
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo
            .push_back(std::mem::replace(&mut self.project, next));
        true
    }
}

fn track_index(project: &Project, id: u64) -> ProjectResult<usize> {
    project
        .tracks
        .iter()
        .position(|track| track.id == id)
        .ok_or_else(|| format!("Unknown track ID: {id}").into())
}

fn pattern_index(project: &Project, id: u64) -> ProjectResult<usize> {
    project
        .patterns
        .iter()
        .position(|pattern| pattern.id == id)
        .ok_or_else(|| format!("Unknown pattern ID: {id}").into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_bar_checks_position_before_changing_project_or_history() {
        let mut editor = ProjectEditor::new(Project::default()).unwrap();
        let insert = EditCommand::InsertBar {
            index: 0,
            bar: ArrangementBar::Main {},
        };
        editor.apply_batch(std::slice::from_ref(&insert)).unwrap();
        let arranged = editor.project().clone();
        assert!(editor.undo());
        assert!(editor
            .apply_batch(&[
                insert,
                EditCommand::InsertBar {
                    index: usize::MAX,
                    bar: ArrangementBar::Rest {}
                }
            ])
            .is_err());
        assert_eq!(editor.project(), &Project::default());
        assert!(editor.redo());
        assert_eq!(editor.project(), &arranged);
    }

    #[test]
    fn batch_uses_stable_ids_and_undo_redo_restores_every_field() {
        let mut original = Project::default();
        original.tracks[0].id = 42;
        let mut editor = ProjectEditor::new(original.clone()).unwrap();
        let commands = [
            EditCommand::SetStep {
                track_id: 42,
                step: 15,
                velocity: 127,
            },
            EditCommand::SetBpm { bpm: 123.5 },
            EditCommand::SetMasterVolume { volume: 0.25 },
            EditCommand::SetSample {
                track_id: 42,
                sample: Some("samples/kick.wav".into()),
            },
        ];
        assert!(editor.apply_batch(&commands).unwrap());
        let changed = editor.project().clone();
        assert_eq!(changed.pattern.get(0, 15), Some(127));
        assert_eq!(changed.bpm, 123.5);
        assert_eq!(changed.master_volume, 0.25);
        assert_eq!(changed.tracks[0].sample, Some("samples/kick.wav".into()));
        assert!(editor.undo());
        assert_eq!(editor.project(), &original);
        assert!(!editor.undo());
        assert!(editor.redo());
        assert_eq!(editor.project(), &changed);
        assert!(!editor.redo());
        assert!(editor
            .apply_batch(&[EditCommand::SetSample {
                track_id: 42,
                sample: None
            }])
            .unwrap());
        assert_eq!(editor.project().tracks[0].sample, None);
    }

    #[test]
    fn invalid_commands_roll_back_the_entire_batch_and_preserve_redo() {
        let mut editor = ProjectEditor::new(Project::default()).unwrap();
        editor
            .apply_batch(&[EditCommand::SetBpm { bpm: 120.0 }])
            .unwrap();
        editor.undo();
        let original = editor.project().clone();
        for invalid in [
            EditCommand::SetBpm { bpm: f64::NAN },
            EditCommand::SetBpm { bpm: 1000.0 },
            EditCommand::SetMasterVolume {
                volume: f32::INFINITY,
            },
            EditCommand::SetMasterVolume { volume: -0.1 },
            EditCommand::SetStep {
                track_id: 99,
                step: 0,
                velocity: 127,
            },
            EditCommand::SetStep {
                track_id: 1,
                step: 16,
                velocity: 127,
            },
            EditCommand::SetStep {
                track_id: 1,
                step: 0,
                velocity: 128,
            },
            EditCommand::SetSample {
                track_id: 1,
                sample: Some("".into()),
            },
            EditCommand::SetSample {
                track_id: 0,
                sample: None,
            },
        ] {
            assert!(editor
                .apply_batch(&[EditCommand::SetBpm { bpm: 90.0 }, invalid])
                .is_err());
            assert_eq!(editor.project(), &original);
        }
        assert!(!editor.apply_batch(&[]).unwrap());
        assert!(!editor
            .apply_batch(&[EditCommand::SetBpm { bpm: original.bpm }])
            .unwrap());
        assert!(editor.redo());
        assert_eq!(editor.project().bpm, 120.0);
    }

    #[test]
    fn new_edits_discard_redo_and_history_is_bounded() {
        let mut editor = ProjectEditor::new(Project::default()).unwrap();
        for i in 0..130 {
            editor
                .apply_batch(&[EditCommand::SetBpm {
                    bpm: 200.0 + i as f64,
                }])
                .unwrap();
        }
        for _ in 0..ProjectEditor::HISTORY_LIMIT {
            assert!(editor.undo());
        }
        assert!(!editor.undo());
        assert_eq!(editor.project().bpm, 201.0);
        assert!(editor.redo());
        editor
            .apply_batch(&[EditCommand::SetBpm { bpm: 90.0 }])
            .unwrap();
        assert!(!editor.redo());
    }

    #[test]
    fn command_json_is_strict_and_invalid_initial_projects_are_rejected() {
        for json in [
            r#"{"command":"set_bpm","bpm":120,"typo":true}"#,
            r#"{"command":"unknown"}"#,
            r#"{"command":"set_step","track_id":1,"step":0,"velocity":256}"#,
            r#"{"command":"set_arrangement","bars":[{"type":"main","typo":true}]}"#,
        ] {
            assert!(serde_json::from_str::<EditCommand>(json).is_err());
        }
        assert!(ProjectEditor::new(Project {
            bpm: 0.0,
            ..Project::default()
        })
        .is_err());
    }
}
