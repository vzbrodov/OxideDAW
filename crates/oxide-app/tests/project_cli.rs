//! End-to-end document workflow; no audio device is needed.
use oxide_core::{sequencer::Pattern, Project};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
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
        // Minimal one-frame PCM WAV, 16-bit mono, 8 kHz.
        let mut wav = Vec::from(&b"RIFF"[..]);
        wav.extend(38_u32.to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16_u32.to_le_bytes());
        wav.extend(1_u16.to_le_bytes());
        wav.extend(1_u16.to_le_bytes());
        wav.extend(8000_u32.to_le_bytes());
        wav.extend(16000_u32.to_le_bytes());
        wav.extend(2_u16.to_le_bytes());
        wav.extend(16_u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(2_u32.to_le_bytes());
        wav.extend(1000_i16.to_le_bytes());
        fs::write(self.0.join("sample.wav"), wav).unwrap();
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
