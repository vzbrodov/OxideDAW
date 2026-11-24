//! CLI audio prototype. All project I/O and decoding precede audio playback.
mod controls;
mod editor;
mod project_pack;
use oxide_audio::export::{export_project_as, WavEncoding};
use oxide_audio::playback::{output_devices, Playback};
use oxide_core::{
    sequencer::{sequence_step_count, Pattern, TRACKS},
    Project,
};
use oxide_dsp::sample::AudioSample;
use std::{
    io::{self, BufRead},
    path::Path,
    sync::atomic::Ordering,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const HELP: &str = "Usage:
  oxide-app gui [project.oxide.json]  (requires --features gui)
  oxide-app devices
  oxide-app play <file.wav> [volume: 0..1] [device name]
  oxide-app pattern <kick.wav> <snare.wav> <hat.wav> <perc.wav> [bpm] [volume: 0..1] [device name]
  oxide-app project-new <project.oxide.json> <kick.wav> <snare.wav> <hat.wav> <perc.wav> [bpm] [volume: 0..1]
  oxide-app project-info <project.oxide.json>
  oxide-app project-play <project.oxide.json> [--arrangement] [device name]
  oxide-app project-export <project.oxide.json> <output.wav> [--format <pcm16|pcm24|float32>]
  oxide-app project-pack <project.oxide.json> <new-directory>
  oxide-app project-edit <project.oxide.json>
pattern and project-new use a preset 16-step beat. Default BPM: 120, volume: 0.5.
play runs once, then outputs silence; patterns loop. Enter stops playback.
Pattern controls: play, pause, stop, bpm <1..999>, seek <zero-based step>, quit.
project-new refuses to overwrite an existing file. Media is referenced, not copied.";

fn main() {
    if let Err(error) = run() {
        eprintln!("OxideDAW: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        #[cfg(feature = "gui")]
        Some("gui") if args.len() <= 2 => oxide_ui::run(args.get(1).map(Path::new))?,
        #[cfg(not(feature = "gui"))]
        Some("gui") => return Err("Build with --features gui to open the desktop interface".into()),
        Some("devices") if args.len() == 1 => {
            for name in output_devices()? {
                println!("{name}");
            }
        }
        Some("play") if (2..=4).contains(&args.len()) => {
            let volume = args
                .get(2)
                .map(|s| s.parse::<f32>())
                .transpose()?
                .unwrap_or(0.5);
            let sample = AudioSample::load(Path::new(&args[1]))?;
            let playback = Playback::start(sample, args.get(3).map(String::as_str), volume)?;
            println!("Playing {}. Press Enter to stop.", args[1]);
            wait_for_stop(playback, false)?;
        }
        Some("pattern") if (5..=8).contains(&args.len()) => {
            let bpm = args
                .get(5)
                .map(|s| s.parse::<f64>())
                .transpose()?
                .unwrap_or(120.0);
            let volume = args
                .get(6)
                .map(|s| s.parse::<f32>())
                .transpose()?
                .unwrap_or(0.5);
            let samples = load_samples(&args[1..5])?;
            let playback = Playback::start_pattern(
                samples,
                Pattern::drum_demo(),
                bpm,
                args.get(7).map(String::as_str),
                volume,
            )?;
            println!("Four-track demo pattern at {bpm} BPM. Controls: play, pause, stop, bpm N, seek N; Enter quits.");
            wait_for_stop(playback, true)?;
        }
        Some("project-new") if (6..=8).contains(&args.len()) => {
            let path = Path::new(&args[1]);
            if path.try_exists()? {
                return Err("Project already exists; choose another file name".into());
            }
            let mut project = Project {
                name: path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Untitled Project")
                    .into(),
                bpm: args
                    .get(6)
                    .map(|s| s.parse::<f64>())
                    .transpose()?
                    .unwrap_or(120.0),
                master_volume: args
                    .get(7)
                    .map(|s| s.parse::<f32>())
                    .transpose()?
                    .unwrap_or(0.5),
                pattern: Pattern::drum_demo(),
                ..Project::default()
            };
            project.validate()?;
            // Validate decodability before publishing a new project file.
            let _ = load_samples(&args[2..6])?;
            for (index, name) in ["Kick", "Snare", "Hi-hat", "Percussion"].iter().enumerate() {
                project.tracks[index].name = (*name).into();
                project.set_sample_path(path, index, Path::new(&args[index + 2]))?;
            }
            project.save_new(path)?;
            println!("Saved {}", path.display());
        }
        Some("project-info") if args.len() == 2 => {
            let path = Path::new(&args[1]);
            let project = Project::load(path)?;
            println!(
                "{} — {} BPM, {} Hz, volume {} (format v{})",
                project.name,
                project.bpm,
                project.sample_rate,
                project.master_volume,
                project.format_version
            );
            for (index, track) in project.tracks.iter().enumerate() {
                let media = match &track.sample {
                    Some(_) => {
                        let media = project.sample_path(path, index)?;
                        let status = if media.is_file() {
                            "available"
                        } else {
                            "missing"
                        };
                        format!("{} ({status})", media.display())
                    }
                    None => "unassigned".into(),
                };
                println!("Track {}: {} — {media}", track.id, track.name);
            }
            if !project.arrangement.is_empty() {
                println!("Arrangement: {} bars", project.arrangement.len());
            }
            for pattern in &project.patterns {
                println!("Pattern {}: {}", pattern.id, pattern.name);
            }
        }
        Some("project-play") if (2..=4).contains(&args.len()) => {
            let path = Path::new(&args[1]);
            let options = &args[2..];
            let arrangement_mode = options.first().is_some_and(|arg| arg == "--arrangement");
            let device_args = if arrangement_mode {
                &options[1..]
            } else {
                options
            };
            if device_args.len() > 1 {
                return Err("Expected at most one output device name".into());
            }
            let device = device_args.first().map(String::as_str);
            let project = Project::load(path)?;
            let samples = load_project_samples(&project, path, arrangement_mode)?;
            let arrangement = if arrangement_mode {
                project.playback_bars()?
            } else {
                Vec::new()
            };
            let last_step = sequence_step_count(&arrangement) - 1;
            let playback = Playback::start_arrangement(
                samples,
                project.pattern,
                arrangement,
                project.bpm,
                device,
                project.master_volume,
            )?;
            println!(
                "Playing {} at {} BPM. Controls: play, pause, stop, bpm N, seek 0..{last_step}; Enter quits.",
                project.name, project.bpm
            );
            wait_for_stop(playback, true)?;
        }
        Some("project-export") if args.len() >= 3 => {
            let encoding = match &args[3..] {
                [] => WavEncoding::default(),
                [flag, value] if flag == "--format" => match value.as_str() {
                    "pcm16" => WavEncoding::Pcm16,
                    "pcm24" => WavEncoding::Pcm24,
                    "float32" => WavEncoding::Float32,
                    _ => return Err("WAV format must be pcm16, pcm24, or float32".into()),
                },
                _ => return Err("Expected --format <pcm16|pcm24|float32> after output path".into()),
            };
            let path = Path::new(&args[1]);
            let destination = Path::new(&args[2]);
            let project = Project::load(path)?;
            let samples = load_project_samples(&project, path, true)?;
            export_project_as(&project, samples, destination, encoding)
                .map_err(|error| format!("Cannot export {}: {error}", destination.display()))?;
            println!("Exported {}", destination.display());
        }
        Some("project-edit") if args.len() == 2 => editor::run(Path::new(&args[1]))?,
        Some("project-pack") if args.len() == 3 => {
            let destination = project_pack::pack(Path::new(&args[1]), Path::new(&args[2]))?;
            println!("Packed {}", destination.display());
        }
        Some("--help") if args.len() == 1 => println!("{HELP}"),
        None => println!("{HELP}"),
        _ => return Err("Invalid arguments. Run with --help".into()),
    }
    Ok(())
}

fn load_samples(paths: &[String]) -> Result<[AudioSample; TRACKS]> {
    let samples: Vec<_> = paths
        .iter()
        .map(|path| {
            AudioSample::load(Path::new(path))
                .map_err(|error| format!("Cannot load sample {path}: {error}").into())
        })
        .collect::<Result<_>>()?;
    samples
        .try_into()
        .map_err(|_| "Expected four samples".into())
}

fn load_project_samples(
    project: &Project,
    path: &Path,
    allow_unassigned: bool,
) -> Result<[AudioSample; TRACKS]> {
    let samples: Vec<_> = project
        .tracks
        .iter()
        .enumerate()
        .map(|(index, track)| {
            if allow_unassigned && track.sample.is_none() {
                return Ok(AudioSample {
                    samples: vec![0.0],
                    channels: 1,
                    sample_rate: project.sample_rate,
                });
            }
            let sample_path = project.sample_path(path, index)?;
            AudioSample::load(&sample_path).map_err(|error| {
                format!(
                    "Track {} ({}): cannot load {}: {error}",
                    track.id,
                    track.name,
                    sample_path.display()
                )
                .into()
            })
        })
        .collect::<Result<_>>()?;
    samples
        .try_into()
        .map_err(|_| "Expected four samples".into())
}

fn wait_for_stop(mut playback: Playback, pattern: bool) -> Result<()> {
    // Keep stream ownership on this thread while input waits elsewhere.
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let finished = line.as_ref().map_or(true, |line| {
                !pattern || line.trim().is_empty() || line.trim() == "quit"
            });
            if sender.send(line).is_err() || finished {
                break;
            }
        }
    });
    let mut rejected = false;
    loop {
        if playback.failed.load(Ordering::Relaxed) {
            return Err("Audio stream failed (device disconnected or backend error)".into());
        }
        match receiver.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(result) => {
                let line = result?;
                if !pattern || line.trim().is_empty() || line.trim() == "quit" {
                    break;
                }
                let result = controls::parse(&line)
                    .map_err(Into::into)
                    .and_then(|command| playback.command(command));
                if let Err(error) = result {
                    eprintln!("Playback command rejected: {error}");
                    rejected = true;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    if rejected {
        return Err("One or more playback commands were rejected".into());
    }
    Ok(())
}
