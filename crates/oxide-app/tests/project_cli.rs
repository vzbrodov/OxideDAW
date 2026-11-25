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
fn export_writes_selected_wav_encoding() {
    let fixture = Fixture::new();
    fixture.wave_samples(2, &[i16::MAX, i16::MIN, 1, -1]);
    let mut project = Project {
        bpm: 120.0,
        sample_rate: 8000,
        master_volume: 1.0,
        ..Project::default()
    };
    for track in 0..2 {
        project.tracks[track].sample = Some("sample.wav".into());
        project.pattern.set(track, 0, 64).unwrap();
    }
    project
        .save_new(&fixture.0.join("beat.oxide.json"))
        .unwrap();

    for (format, tag, bits, data_offset) in [
        ("pcm16", 1, 16, 44),
        ("pcm24", 1, 24, 44),
        ("float32", 3, 32, 58),
    ] {
        let name = format!("{format}.wav");
        let args = [
            "project-export",
            "beat.oxide.json",
            &name,
            "--format",
            format,
        ];
        let result = fixture.run(&args);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );

        let path = fixture.0.join(&name);
        let bytes = fs::read(&path).unwrap();
        let u32_at = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        assert_eq!(u32_at(4) as usize, bytes.len() - 8);
        assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), tag);
        assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), bits);
        assert_eq!(&bytes[data_offset - 8..data_offset - 4], b"data");
        assert_eq!(u32_at(data_offset - 4) as usize, bytes.len() - data_offset);

        let decoded = oxide_dsp::sample::AudioSample::load(&path).unwrap();
        assert_eq!((decoded.channels, decoded.sample_rate), (2, 8000));
        assert_eq!(decoded.samples.len(), 32000);
        assert_eq!(decoded.samples[1], -1.0);
        match format {
            "pcm16" => {
                assert_eq!(decoded.samples[0], 32767.0 / 32768.0);
                assert_eq!(decoded.samples[2], 1.0 / 32768.0);
            }
            "pcm24" => {
                assert_eq!(decoded.samples[0], 8388607.0 / 8388608.0);
                assert_eq!(decoded.samples[2], 258.0 / 8388608.0);
            }
            "float32" => {
                assert_eq!(decoded.samples[0], 1.0);
                assert_eq!(decoded.samples[2], (128.0 / 127.0) / 32768.0);
                assert_eq!(&bytes[38..42], b"fact");
                assert_eq!(u32_at(46) as usize, decoded.samples.len() / 2);
            }
            _ => unreachable!(),
        }
        assert!(!fixture.run(&args).status.success());
        assert_eq!(fs::read(path).unwrap(), bytes);
    }

    let bad = fixture.run(&[
        "project-export",
        "beat.oxide.json",
        "bad.wav",
        "--format",
        "opus",
    ]);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("WAV format must be"));
    assert!(!fixture.0.join("bad.wav").exists());
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

    let mut v2 = stored;
    v2["format_version"] = 2.into();
    fs::write(&path, v2.to_string()).unwrap();
    assert_eq!(Project::load(&path).unwrap(), saved);

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

#[test]
fn named_pattern_edits_update_every_linked_bar_after_reopening() {
    let fixture = Fixture::new();
    fixture.wave_samples(1, &[8192]);
    let path = fixture.0.join("beat.oxide.json");
    let mut project = Project {
        bpm: 120.0,
        sample_rate: 8000,
        master_volume: 1.0,
        ..Project::default()
    };
    project.tracks[0].sample = Some("sample.wav".into());
    project.save_new(&path).unwrap();
    let result = fixture.edit(concat!(
        r#"[{"command":"add_pattern","pattern_id":42,"name":"Verse"},"#,
        r#"{"command":"insert_bar","index":0,"bar":{"type":"pattern","pattern_id":42}},"#,
        r#"{"command":"insert_bar","index":1,"bar":{"type":"pattern","pattern_id":42}},"#,
        r#"{"command":"insert_bar","index":1,"bar":{"type":"rest"}}]"#,
        "\nundo\nredo\nsave\nquit\n",
    ));
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let initial = Project::load(&path).unwrap();
    let result = fixture.edit(concat!(
        r#"[{"command":"rename_pattern","pattern_id":42,"name":"Discarded"},"#,
        r#"{"command":"remove_pattern","pattern_id":42}]"#,
        "\nsave\nquit\n",
    ));
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unknown pattern ID"));
    assert_eq!(Project::load(&path).unwrap(), initial);

    let result = fixture.edit(concat!(
        r#"{"command":"set_pattern_step","pattern_id":42,"track_id":1,"step":0,"velocity":127}"#,
        "\nsave\nquit\n",
    ));
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result = fixture.run(&["project-export", "beat.oxide.json", "song.wav"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let audio = oxide_dsp::sample::AudioSample::load(&fixture.0.join("song.wav")).unwrap();
    assert_eq!(audio.samples.len(), 48000 * 2);
    let left_hits: Vec<_> = audio
        .samples
        .iter()
        .step_by(2)
        .enumerate()
        .filter(|&(_, &sample)| sample != 0.0)
        .map(|(frame, &sample)| (frame, sample))
        .collect();
    assert_eq!(left_hits, [(0, 0.25), (32000, 0.25)]);
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

#[test]
fn stdio_mcp_reads_project_and_recovers_from_invalid_messages() {
    let fixture = Fixture::new();
    let path = fixture.0.join("beat.oxide.json");
    let project = Project {
        bpm: 121.5,
        ..Project::default()
    };
    project.save_new(&path).unwrap();
    let original = fs::read(&path).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_oxide-app"))
        .arg("mcp")
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    writeln!(input, "{}", "x".repeat(70_000)).unwrap();
    input.write_all(br#"invalid json
{"jsonrpc":"2.0","id":1,"method":"tools/list"}
{"jsonrpc":"2.0","id":"init","method":"initialize","params":{"protocolVersion":"future","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list"}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_project_summary","arguments":{"path":"elsewhere"}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"get_project_summary","arguments":{}}}
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"save_project","arguments":{}}}
"#).unwrap();
    drop(input);
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let replies: Vec<serde_json::Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(replies.len(), 8);
    assert_eq!(replies[4]["result"]["tools"].as_array().unwrap().len(), 2);
    assert_eq!(replies[7]["error"]["code"], -32602);
    assert_eq!(replies[0]["error"]["code"], -32600);
    assert_eq!(replies[1]["error"]["code"], -32700);
    assert_eq!(replies[2]["error"]["code"], -32002);
    assert_eq!(replies[3]["id"], "init");
    assert_eq!(replies[3]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(
        replies[4]["result"]["tools"][0]["name"],
        "get_project_summary"
    );
    assert_eq!(replies[5]["error"]["code"], -32602);
    let summary = &replies[6]["result"]["structuredContent"];
    assert_eq!(summary["bpm"], 121.5);
    assert_eq!(summary["tracks"][0]["id"], project.tracks[0].id);
    assert_eq!(fs::read(path).unwrap(), original);
}

#[test]
fn editable_mcp_batches_undo_and_saves_only_on_request() {
    use serde_json::{json, Value};
    use std::io::{BufRead, BufReader};

    let fixture = Fixture::new();
    let path = fixture.0.join("beat.oxide.json");
    let initial = Project::default();
    initial.save_new(&path).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_oxide-app"))
        .arg("mcp")
        .arg(&path)
        .arg("--edit")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    writeln!(input, "{}", json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{
        "protocolVersion":"2025-06-18","clientInfo":{"name":"test","version":"1"},"capabilities":{}}})).unwrap();
    reader.read_line(&mut line).unwrap();
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0","method":"notifications/initialized"})
    )
    .unwrap();
    {
        let mut id = 0;
        let mut call = |name: &str, arguments: Value| {
            id += 1;
            writeln!(input,"{}", json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":arguments}})).unwrap();
            line.clear();
            reader.read_line(&mut line).unwrap();
            serde_json::from_str::<Value>(&line).unwrap()
        };
        let step = |index, velocity| json!({"track_id":1,"step":index,"velocity":velocity});
        let batch = json!({"steps":[step(0,80),step(15,110)]});
        assert_eq!(
            call("set_main_steps", batch)["result"]["structuredContent"]["changed"],
            true
        );
        assert_eq!(
            call("undo", json!({}))["result"]["structuredContent"]["changed"],
            true
        );
        let grid = call("get_main_pattern", json!({}));
        assert_eq!(
            grid["result"]["structuredContent"]["pattern"],
            json!(initial.pattern)
        );
        let invalid = json!({"steps":[step(2,100),step(16,90)]});
        assert_eq!(
            call("set_main_steps", invalid)["result"]["structuredContent"]["code"],
            "edit_rejected"
        );
        let oversized = json!({"steps":vec![step(0,1);65]});
        assert_eq!(call("set_main_steps", oversized)["error"]["code"], -32602);
        assert_eq!(
            call("redo", json!({}))["result"]["structuredContent"]["changed"],
            true
        );
        assert_eq!(Project::load(&path).unwrap(), initial);
        assert_eq!(
            call("save_project", json!({}))["result"]["structuredContent"]["saved"],
            true
        );
    }
    drop(input);
    assert!(child.wait().unwrap().success());
    let mut expected = initial;
    expected.pattern.set(0, 0, 80).unwrap();
    expected.pattern.set(0, 15, 110).unwrap();
    assert_eq!(Project::load(&path).unwrap(), expected);
}
