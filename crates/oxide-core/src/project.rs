//! Versioned project documents. File I/O and validation belong on the control thread.
use crate::sequencer::{ArrangementBar, Pattern, PlaybackBar, Transport, TRACKS};
use crate::storage::{parent_directory, write_file};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const FORMAT_VERSION: u32 = 3;
pub const MAX_NAMED_PATTERNS: usize = 256;
pub const MAX_PATTERN_NAME_BYTES: usize = 128;
const MAX_PROJECT_BYTES: usize = 16 * 1024 * 1024;
const PROJECT_TOO_LARGE: &str = "Project JSON exceeds the 16 MiB size limit";
pub type ProjectResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Track {
    /// Stable identity, independent of the display name. Zero is reserved.
    pub id: u64,
    pub name: String,
    /// Relative to the project file's directory, or an absolute external path.
    /// None permits editing and saving a project before choosing a sample.
    pub sample: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedPattern {
    pub id: u64,
    pub name: String,
    pub pattern: Pattern,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub format_version: u32,
    pub name: String,
    pub bpm: f64,
    /// Target offline rate; live playback follows the selected audio device.
    pub sample_rate: u32,
    pub master_volume: f32,
    /// Track slots correspond to rows in the pattern grid.
    pub tracks: [Track; TRACKS],
    pub pattern: Pattern,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patterns: Vec<NamedPattern>,
    /// Empty uses the main pattern by itself.
    #[serde(default)]
    pub arrangement: Vec<ArrangementBar>,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            name: "Untitled Project".into(),
            bpm: 140.0,
            sample_rate: 44100,
            master_volume: 0.5,
            tracks: std::array::from_fn(|index| Track {
                id: index as u64 + 1,
                name: format!("Track {}", index + 1),
                sample: None,
            }),
            pattern: Pattern::default(),
            patterns: Vec::new(),
            arrangement: Vec::new(),
        }
    }
}

impl Project {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.format_version != FORMAT_VERSION {
            return Err("Unsupported project format version");
        }
        if self.name.trim().is_empty() {
            return Err("Project name must not be empty");
        }
        Transport::new(self.sample_rate, self.bpm)?;
        if !self.master_volume.is_finite() || !(0.0..=1.0).contains(&self.master_volume) {
            return Err("Master volume must be between 0 and 1");
        }
        for (index, track) in self.tracks.iter().enumerate() {
            if track.id == 0
                || self.tracks[..index]
                    .iter()
                    .any(|other| other.id == track.id)
            {
                return Err("Track IDs must be nonzero and unique");
            }
            if track.name.trim().is_empty() {
                return Err("Track name must not be empty");
            }
            if let Some(path) = &track.sample {
                if path.as_os_str().is_empty() || path.to_str().is_none() {
                    return Err("Sample paths must be nonempty UTF-8 paths");
                }
            }
        }
        if self.patterns.len() > MAX_NAMED_PATTERNS {
            return Err("Project may contain at most 256 named patterns");
        }
        let mut ids = HashSet::with_capacity(self.patterns.len());
        for pattern in &self.patterns {
            if pattern.id == 0 || !ids.insert(pattern.id) {
                return Err("Pattern IDs must be nonzero and unique");
            }
            if pattern.name.trim().is_empty() || pattern.name.len() > MAX_PATTERN_NAME_BYTES {
                return Err("Pattern names must be nonempty and at most 128 UTF-8 bytes");
            }
        }
        for bar in &self.arrangement {
            if let ArrangementBar::Pattern { pattern_id } = bar {
                if !ids.contains(pattern_id) {
                    return Err("Arrangement references an unknown pattern ID");
                }
            }
        }
        Ok(())
    }

    /// Prepare self-contained bars for the shared live and offline renderer.
    pub fn playback_bars(&self) -> ProjectResult<Vec<PlaybackBar>> {
        let patterns: HashMap<_, _> = self.patterns.iter().map(|p| (p.id, &p.pattern)).collect();
        self.arrangement
            .iter()
            .map(|bar| {
                Ok(match bar {
                    ArrangementBar::Main {} => PlaybackBar::Main {},
                    ArrangementBar::Rest {} => PlaybackBar::Rest {},
                    ArrangementBar::Variation { pattern } => PlaybackBar::Variation {
                        pattern: pattern.clone(),
                    },
                    ArrangementBar::Pattern { pattern_id } => {
                        let pattern = patterns.get(pattern_id).ok_or_else(|| {
                            format!("Arrangement references unknown pattern ID {pattern_id}")
                        })?;
                        PlaybackBar::Variation {
                            pattern: (*pattern).clone(),
                        }
                    }
                })
            })
            .collect()
    }

    pub fn from_json(json: &str) -> ProjectResult<Self> {
        if json.len() > MAX_PROJECT_BYTES {
            return Err(PROJECT_TOO_LARGE.into());
        }
        let mut project: Self = serde_json::from_str(json)?;
        if matches!(project.format_version, 1 | 2) {
            if (project.format_version == 1 && !project.arrangement.is_empty())
                || !project.patterns.is_empty()
                || project
                    .arrangement
                    .iter()
                    .any(|bar| matches!(bar, ArrangementBar::Pattern { .. }))
            {
                return Err("Project contains features unavailable in its format version".into());
            }
            project.format_version = FORMAT_VERSION;
        }
        project.validate()?;
        Ok(project)
    }

    pub fn to_json(&self) -> ProjectResult<String> {
        self.validate()?;
        let json = serde_json::to_string_pretty(self)? + "\n";
        if json.len() > MAX_PROJECT_BYTES {
            return Err(PROJECT_TOO_LARGE.into());
        }
        Ok(json)
    }

    /// Missing media does not prevent loading the musical document.
    pub fn load(path: &Path) -> ProjectResult<Self> {
        let file = fs::File::open(path)?;
        if file.metadata()?.len() > MAX_PROJECT_BYTES as u64 {
            return Err(PROJECT_TOO_LARGE.into());
        }
        // Bound the read even if the file grows after checking metadata.
        let mut bytes = Vec::new();
        file.take(MAX_PROJECT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_PROJECT_BYTES {
            return Err(PROJECT_TOO_LARGE.into());
        }
        Self::from_json(std::str::from_utf8(&bytes)?)
    }

    /// Resolve media without using the process working directory as its base.
    pub fn sample_path(&self, project_file: &Path, track: usize) -> ProjectResult<PathBuf> {
        let track = self.tracks.get(track).ok_or("Invalid track index")?;
        let sample = track
            .sample
            .as_ref()
            .ok_or_else(|| format!("Track {} ({}) has no sample assigned", track.id, track.name))?;
        if sample.is_absolute() {
            return Ok(sample.clone());
        }
        Ok(parent_directory(project_file).join(sample))
    }

    /// Store a portable reference for media inside the project directory.
    /// External media stays absolute; copying media into the project is explicit.
    pub fn set_sample_path(
        &mut self,
        project_file: &Path,
        track: usize,
        sample: &Path,
    ) -> ProjectResult<()> {
        let base = fs::canonicalize(parent_directory(project_file))?;
        let sample = fs::canonicalize(sample)?;
        if !sample.is_file() {
            return Err("Expected an audio file".into());
        }
        let stored = sample.strip_prefix(&base).unwrap_or(&sample).to_path_buf();
        if stored.to_str().is_none() {
            return Err("Sample paths must be UTF-8".into());
        }
        self.tracks
            .get_mut(track)
            .ok_or("Invalid track index")?
            .sample = Some(stored);
        Ok(())
    }

    /// Write and sync a sibling temporary file, then atomically replace the target.
    /// Existing data is untouched if validation, writing or renaming fails.
    /// Relative media references keep their spelling: moving to a new directory
    /// requires moving the media as well, or explicitly rebasing those references.
    pub fn save(&self, path: &Path) -> ProjectResult<()> {
        self.write_document(path, true)
    }

    /// Atomically create a document, refusing to replace any existing entry.
    /// Uses a sibling hard link so another writer cannot race the existence check.
    pub fn save_new(&self, path: &Path) -> ProjectResult<()> {
        self.write_document(path, false)
    }

    fn write_document(&self, path: &Path, replace: bool) -> ProjectResult<()> {
        let json = self.to_json()?;
        if path.file_name().is_none() {
            return Err("Expected a project file path".into());
        }
        write_file(path, replace, |file| file.write_all(json.as_bytes()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_ID: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "oxidedaw-project-test-{}-{}",
                std::process::id(),
                TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn project_size_limit_preserves_existing_document() {
        let dir = TestDirectory::new();
        let path = dir.0.join("project.oxide.json");
        let project = Project::default();
        let mut json = project.to_json().unwrap().into_bytes();
        json.resize(MAX_PROJECT_BYTES, b' ');
        fs::write(&path, &json).unwrap();
        assert_eq!(Project::load(&path).unwrap(), project);

        let oversized = Project {
            name: "x".repeat(MAX_PROJECT_BYTES),
            ..project
        };
        assert_eq!(
            oversized.save(&path).unwrap_err().to_string(),
            PROJECT_TOO_LARGE
        );
        assert_eq!(fs::read(&path).unwrap(), json);

        json.push(b' ');
        fs::write(&path, &json).unwrap();
        assert_eq!(
            Project::load(&path).unwrap_err().to_string(),
            PROJECT_TOO_LARGE
        );
    }

    #[test]
    fn round_trip_preserves_ids_media_and_all_velocities() {
        let mut project = Project {
            name: "Ритм".into(),
            bpm: 137.25,
            master_volume: 0.3,
            ..Project::default()
        };
        for (index, track) in project.tracks.iter_mut().enumerate() {
            track.id = 100 + index as u64;
            track.sample = Some(PathBuf::from(format!("samples/удар {index}.wav")));
            for step in 0..16 {
                project
                    .pattern
                    .set(index, step, (index * 16 + step) as u8)
                    .unwrap();
            }
        }
        let loaded = Project::from_json(&project.to_json().unwrap()).unwrap();
        assert_eq!(loaded, project);
        assert_eq!(
            loaded
                .sample_path(Path::new("session/beat.oxide.json"), 0)
                .unwrap(),
            Path::new("session/samples/удар 0.wav")
        );
    }

    #[test]
    fn rejects_bad_version_tempo_ids_velocity_grid_and_unknown_fields() {
        let base = serde_json::to_value(Project::default()).unwrap();
        for (key, value) in [
            ("format_version", serde_json::json!(FORMAT_VERSION + 1)),
            ("bpm", serde_json::json!(0)),
            ("master_volume", serde_json::json!(1.1)),
            ("sample_rate", serde_json::json!(0)),
            ("name", serde_json::json!(" ")),
            ("typo", serde_json::json!(true)),
        ] {
            let mut json = base.clone();
            json[key] = value;
            assert!(Project::from_json(&json.to_string()).is_err(), "{key}");
        }
        let mut json = base.clone();
        json["tracks"][1]["id"] = serde_json::json!(1);
        assert!(Project::from_json(&json.to_string()).is_err());
        let mut json = base.clone();
        json["pattern"]["steps"][0][0] = serde_json::json!(128);
        assert!(Project::from_json(&json.to_string()).is_err());
        let mut json = base.clone();
        json["pattern"]["steps"][0] = serde_json::json!([127]);
        assert!(Project::from_json(&json.to_string()).is_err());
        let invalid = Project {
            bpm: f64::NAN,
            ..Project::default()
        };
        assert!(invalid.to_json().is_err());
        assert!(Project::from_json("{truncated").is_err());
    }

    #[test]
    fn atomic_save_replaces_complete_document_and_preserves_it_on_failure() {
        let dir = TestDirectory::new();
        let path = dir.0.join("beat.oxide.json");
        let mut project = Project::default();
        project.save(&path).unwrap();
        project.pattern = Pattern::drum_demo();
        project.save(&path).unwrap();
        assert_eq!(Project::load(&path).unwrap(), project);
        project.master_volume = f32::NAN;
        assert!(project.save(&path).is_err());
        assert_eq!(Project::load(&path).unwrap().pattern, Pattern::drum_demo());
        // A rename failure cleans its temporary file and preserves the target.
        let occupied = dir.0.join("directory");
        fs::create_dir(&occupied).unwrap();
        assert!(Project::default().save(&occupied).is_err());
        assert!(occupied.is_dir());
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 2);
    }

    #[test]
    fn media_references_survive_moving_a_project_directory() {
        let dir = TestDirectory::new();
        let original = dir.0.join("original");
        fs::create_dir(&original).unwrap();
        let sample = original.join("kick.wav");
        fs::write(&sample, b"placeholder").unwrap();
        let path = original.join("beat.oxide.json");
        let mut project = Project::default();
        project.set_sample_path(&path, 0, &sample).unwrap();
        assert_eq!(project.tracks[0].sample, Some(PathBuf::from("kick.wav")));
        project.save(&path).unwrap();
        let moved = dir.0.join("moved");
        fs::rename(&original, &moved).unwrap();
        let moved_file = moved.join("beat.oxide.json");
        let loaded = Project::load(&moved_file).unwrap();
        assert!(loaded.sample_path(&moved_file, 0).unwrap().is_file());
        fs::remove_file(moved.join("kick.wav")).unwrap();
        assert!(Project::load(&moved_file).is_ok());
        assert!(!loaded.sample_path(&moved_file, 0).unwrap().exists());
        assert!(loaded.sample_path(&moved_file, 1).is_err());
    }

    #[test]
    fn creating_a_project_never_replaces_an_existing_document() {
        let dir = TestDirectory::new();
        let path = dir.0.join("beat.oxide.json");
        let original = Project::default();
        original.save_new(&path).unwrap();
        let changed = Project {
            name: "New beat".into(),
            ..Project::default()
        };
        assert!(changed.save_new(&path).is_err());
        assert_eq!(Project::load(&path).unwrap(), original);
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    }
}
