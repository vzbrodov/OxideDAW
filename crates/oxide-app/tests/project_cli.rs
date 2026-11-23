//! End-to-end document workflow; no audio device is needed.
use oxide_core::{
    project::FORMAT_VERSION,
    sequencer::{ArrangementBar, Pattern},
    Project,
};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "oxidedaw-cli-test-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_oxide-app"))
            .args(args)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
    fn wave(&self) {
        self.wave_samples(1, &[1000]);
    }
    fn wave_samples(&self, channels: u16, samples: &[i16]) {
        let data_size = samples.len() as u32 * 2;
        let mut wav = Vec::from(&b"RIFF"[..]);
        wav.extend((36 + data_size).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16_u32.to_le_bytes());
        wav.extend(1_u16.to_le_bytes());
        wav.extend(channels.to_le_bytes());
        wav.extend(8000_u32.to_le_bytes());
        wav.extend((16000 * u32::from(channels)).to_le_bytes());
        wav.extend((channels * 2).to_le_bytes());
        wav.extend(16_u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(data_size.to_le_bytes());
        wav.extend(samples.iter().flat_map(|sample| sample.to_le_bytes()));
        fs::write(self.0.join("sample.wav"), wav).unwrap();
    }
    fn edit(&self, input: &str) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_oxide-app"))
            .args(["project-edit", "beat.oxide.json"])
            .current_dir(&self.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}

#[test]
fn packed_project_renders_identically_after_move() {
    let fixture = Fixture::new();
    let root = &fixture.0;
    let relative = root.join("relative");
    let external = root.join("external");
    fs::create_dir(&relative).unwrap();
    fs::create_dir(&external).unwrap();
    fixture.wave_samples(1, &[4096; 256]);
    fs::rename(root.join("sample.wav"), relative.join("sample.wav")).unwrap();
    fixture.wave_samples(1, &[-8192; 512]);
    let absolute_sample = external.join("sample.wav");
    fs::rename(root.join("sample.wav"), &absolute_sample).unwrap();

    let mut project = Project {
        pattern: Pattern::drum_demo(),
        arrangement: vec![ArrangementBar::Main {}, ArrangementBar::Rest {}],
        sample_rate: 8000,
        ..Project::default()
    };
    project.tracks[0].sample = Some("relative/sample.wav".into());
    project.tracks[1].sample = Some(absolute_sample.clone());
    let source = root.join("beat.oxide.json");
    project.save_new(&source).unwrap();
    let original = fs::read(&source).unwrap();
    assert!(fixture
        .run(&["project-export", "beat.oxide.json", "before.wav"])
        .status
        .success());

    let args = ["project-pack", "beat.oxide.json", "packed"];
    let result = fixture.run(&args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read(&source).unwrap(), original);
    assert!(!fixture.run(&args).status.success());
    fs::rename(root.join("packed"), root.join("moved")).unwrap();
    fs::remove_file(relative.join("sample.wav")).unwrap();
    fs::remove_file(absolute_sample).unwrap();

    let packed = Project::load(&root.join("moved/project.oxide.json")).unwrap();
    project.tracks[0].sample = Some("media/track-1.wav".into());
    project.tracks[1].sample = Some("media/track-2.wav".into());
    assert_eq!(packed, project);
    assert!(fixture
        .run(&["project-export", "moved/project.oxide.json", "after.wav"])
        .status
        .success());
    assert_eq!(
        fs::read(root.join("before.wav")).unwrap(),
        fs::read(root.join("after.wav")).unwrap()
    );
}

#[test]
fn pack_reports_missing_media_without_creating_the_destination() {
    let fixture = Fixture::new();
    fixture.wave();
    let mut project = Project::default();
    project.tracks[0].sample = Some("sample.wav".into());
    project.tracks[1].sample = Some("missing.wav".into());
    project
        .save_new(&fixture.0.join("beat.oxide.json"))
        .unwrap();

    let result = fixture.run(&["project-pack", "beat.oxide.json", "packed"]);
    assert_eq!(result.status.code(), Some(1));
    let message = String::from_utf8_lossy(&result.stderr);
    assert!(
        message.contains("Track 2") && message.contains("missing.wav"),
        "{message}"
    );
    assert!(!fixture.0.join("packed").exists());
}

#[test]
fn export_resamples_and_drains_the_last_hit_at_fractional_tempo() {
    let fixture = Fixture::new();
    fixture.wave_samples(2, &[8192, -16384].repeat(1550));
    let mut project = Project {
        bpm: 137.25,
        sample_rate: 16000,
        master_volume: 0.5,
        ..Project::default()
    };
    project.tracks[0].sample = Some("sample.wav".into());
    project.pattern.set(0, 15, 64).unwrap();
    project.pattern.set(1, 0, 127).unwrap(); // Unassigned channel.
    project
        .save_new(&fixture.0.join("beat.oxide.json"))
        .unwrap();

    let args = ["project-export", "beat.oxide.json", "beat.wav"];
    let result = fixture.run(&args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let path = fixture.0.join("beat.wav");
    let exported = oxide_dsp::sample::AudioSample::load(&path).unwrap();
    assert_eq!(exported.channels, 2);
    assert_eq!(exported.sample_rate, 16000);
    let hit_start = 26230;
    let end = hit_start + 3100;
    assert_eq!(exported.samples.len(), end * 2);
    assert!(exported.samples[..hit_start * 2].iter().all(|&s| s == 0.0));
    let (tail, remainder) = exported.samples[hit_start * 2..].as_chunks::<2>();
    assert!(remainder.is_empty());
    assert!(tail
        .iter()
        .all(|&frame| frame == [2064.0 / 32768.0, -4128.0 / 32768.0]));

    let original = fs::read(&path).unwrap();
    assert!(!fixture.run(&args).status.success());
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn export_keeps_a_full_silent_bar_and_reports_missing_assigned_media() {
    let fixture = Fixture::new();
    let path = fixture.0.join("beat.oxide.json");
    let mut project = Project {
        bpm: 120.0,
        sample_rate: 8000,
        ..Project::default()
    };
    project.save_new(&path).unwrap();
    assert!(fixture
        .run(&["project-export", "beat.oxide.json", "silent.wav"])
        .status
        .success());
    let exported = oxide_dsp::sample::AudioSample::load(&fixture.0.join("silent.wav")).unwrap();
    assert_eq!(exported.samples, vec![0.0; 16000 * 2]);

    project.tracks[0].sample = Some("missing.wav".into());
    project.save(&path).unwrap();
    let result = fixture.run(&["project-export", "beat.oxide.json", "missing.wav"]);
    assert_eq!(result.status.code(), Some(1));
    let message = String::from_utf8_lossy(&result.stderr);
    assert!(
        message.contains("Track 1") && message.contains("missing.wav"),
        "{message}"
    );
    assert!(!fixture.0.join("missing.wav").exists());
}

#[test]
fn legacy_project_can_save_undo_and_export_an_arrangement() {
    let fixture = Fixture::new();
    fixture.wave_samples(1, &[8192; 1024]);
    let path = fixture.0.join("beat.oxide.json");
    let mut original = Project {
        bpm: 137.0,
        sample_rate: 8000,
        master_volume: 1.0,
        ..Project::default()
    };
    original.tracks[0].sample = Some("sample.wav".into());
    original.pattern.set(0, 0, 127).unwrap();
    original.pattern.set(0, 15, 127).unwrap();
    let mut legacy = serde_json::to_value(&original).unwrap();
    legacy["format_version"] = 1.into();
    legacy.as_object_mut().unwrap().remove("arrangement");
    fs::write(&path, legacy.to_string()).unwrap();
    assert_eq!(Project::load(&path).unwrap(), original);

    let mut variation = Pattern::default();
    variation.set(0, 0, 64).unwrap();
    let arrangement = vec![
        ArrangementBar::Main {},
        ArrangementBar::Rest {},
        ArrangementBar::Variation { pattern: variation },
        ArrangementBar::Main {},
    ];
    let command = serde_json::json!({"command": "set_arrangement", "bars": arrangement});
    let result = fixture.edit(&format!("{command}\nundo\nsave\nquit\n"));
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(Project::load(&path).unwrap(), original);
    let result = fixture.edit(&format!("{command}\nundo\nredo\nsave\nquit\n"));
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let saved = Project::load(&path).unwrap();
    assert_eq!(saved.arrangement, arrangement);
    let stored: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(stored["format_version"], FORMAT_VERSION);

    let result = fixture.run(&["project-export", "beat.oxide.json", "song.wav"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let audio = oxide_dsp::sample::AudioSample::load(&fixture.0.join("song.wav")).unwrap();
    let (frames, remainder) = audio.samples.as_chunks::<2>();
    assert!(remainder.is_empty());
    // The last hit rings past frame 56058, the rounded end of bar four.
    assert_eq!(frames.len(), 55182 + 1024);
    let hits = [
        (0, 8192),
        (13139, 8192),
        (28029, 4128),
        (42044, 8192),
        (55182, 8192),
    ];
    for (index, frame) in frames.iter().enumerate() {
        let expected = hits
            .iter()
            .find(|&&(start, _)| (start..start + 1024).contains(&index))
            .map_or(0.0, |&(_, pcm)| pcm as f32 / 32768.0);
        assert_eq!(*frame, [expected; 2], "at frame {index}");
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn creates_portable_project_and_reports_missing_media_after_move() {
    let fixture = Fixture::new();
    fixture.wave();
    let args = [
        "project-new",
        "beat.oxide.json",
        "sample.wav",
        "sample.wav",
        "sample.wav",
        "sample.wav",
        "137.25",
        "0.3",
    ];
    let created = fixture.run(&args);
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let path = fixture.0.join("beat.oxide.json");
    let project = Project::load(&path).unwrap();
    assert_eq!(project.bpm, 137.25);
    assert_eq!(project.master_volume, 0.3);
    assert_eq!(project.pattern, Pattern::drum_demo());
    assert!(project
        .tracks
        .iter()
        .all(|track| track.sample.as_deref() == Some(Path::new("sample.wav"))));
    let before = fs::read(&path).unwrap();
    assert!(!fixture.run(&args).status.success());
    assert_eq!(fs::read(&path).unwrap(), before);

    let moved = fixture.0.join("moved");
    fs::create_dir(&moved).unwrap();
    fs::rename(&path, moved.join("beat.oxide.json")).unwrap();
    fs::rename(fixture.0.join("sample.wav"), moved.join("sample.wav")).unwrap();
    let info = fixture.run(&["project-info", "moved/beat.oxide.json"]);
    assert!(info.status.success());
    assert_eq!(
        String::from_utf8_lossy(&info.stdout)
            .matches("available")
            .count(),
        4
    );
    fs::remove_file(moved.join("sample.wav")).unwrap();
    let info = fixture.run(&["project-info", "moved/beat.oxide.json"]);
    assert!(info.status.success());
    assert_eq!(
        String::from_utf8_lossy(&info.stdout)
            .matches("missing")
            .count(),
        4
    );
    let play = fixture.run(&["project-play", "moved/beat.oxide.json"]);
    assert_eq!(play.status.code(), Some(1));
    let message = String::from_utf8_lossy(&play.stderr);
    assert!(message.contains("Track 1 (Kick)"), "{message}");
    assert!(message.contains("sample.wav"), "{message}");
}

#[test]
fn invalid_parameters_and_unreadable_samples_do_not_create_a_document() {
    let fixture = Fixture::new();
    fixture.wave();
    for args in [
        [
            "project-new",
            "bad.oxide.json",
            "sample.wav",
            "sample.wav",
            "sample.wav",
            "sample.wav",
            "NaN",
        ],
        [
            "project-new",
            "bad.oxide.json",
            "absent.wav",
            "sample.wav",
            "sample.wav",
            "sample.wav",
            "120",
        ],
    ] {
        assert_eq!(fixture.run(&args).status.code(), Some(1));
        assert!(!fixture.0.join("bad.oxide.json").exists());
    }
    fs::write(fixture.0.join("broken.oxide.json"), b"{truncated").unwrap();
    assert_eq!(
        fixture
            .run(&["project-info", "broken.oxide.json"])
            .status
            .code(),
        Some(1)
    );
}

#[test]
fn editor_batches_undo_redo_and_explicit_save_survive_reopening() {
    let fixture = Fixture::new();
    let path = fixture.0.join("beat.oxide.json");
    let original = Project::default();
    original.save_new(&path).unwrap();
    let batch = r#"[{"command":"set_bpm","bpm":137.25},{"command":"set_step","track_id":1,"step":15,"velocity":99},{"command":"set_master_volume","volume":0.3},{"command":"set_sample","track_id":1,"sample":"samples/kick.wav"}]"#;
    let edited = fixture.edit(&format!("{batch}\nundo\nsave\nquit\n"));
    assert!(
        edited.status.success(),
        "{}",
        String::from_utf8_lossy(&edited.stderr)
    );
    assert_eq!(Project::load(&path).unwrap(), original);
    let edited = fixture.edit(&format!("{batch}\nundo\nredo\nshow\nsave\nquit\n"));
    assert!(
        edited.status.success(),
        "{}",
        String::from_utf8_lossy(&edited.stderr)
    );
    let changed = Project::load(&path).unwrap();
    assert_eq!(changed.bpm, 137.25);
    assert_eq!(changed.pattern.get(0, 15), Some(99));
    assert_eq!(changed.master_volume, 0.3);
    assert_eq!(changed.tracks[0].sample, Some("samples/kick.wav".into()));
    // EOF and quit both discard unsaved edits, even after a previous save.
    for ending in ["", "quit\n"] {
        assert!(fixture
            .edit(&format!("{{\"command\":\"set_bpm\",\"bpm\":90}}\n{ending}"))
            .status
            .success());
        assert_eq!(Project::load(&path).unwrap(), changed);
    }
}

#[test]
fn editor_rejects_malformed_and_invalid_batches_without_partial_saves() {
    let fixture = Fixture::new();
    let path = fixture.0.join("beat.oxide.json");
    let original = Project::default();
    original.save_new(&path).unwrap();
    let result = fixture.edit(concat!(
        "{broken\n",
        "[{\"command\":\"set_bpm\",\"bpm\":90},{\"command\":\"set_step\",\"track_id\":1,\"step\":16,\"velocity\":99}]\n",
        "save\nquit\n"
    ));
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(Project::load(&path).unwrap(), original);
    assert_eq!(
        String::from_utf8_lossy(&result.stderr)
            .matches("Edit rejected:")
            .count(),
        2
    );
}
