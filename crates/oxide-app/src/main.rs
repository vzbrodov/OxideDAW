//! CLI audio prototype. All decoding happens before starting the stream.
use oxide_audio::playback::{output_devices, Playback};
use oxide_core::sequencer::{Pattern, STEPS};
use oxide_dsp::sample::AudioSample;
use std::{io, path::Path, sync::atomic::Ordering};

fn main() {
    if let Err(error) = run() {
        eprintln!("OxideDAW: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("devices") if args.len() == 1 => {
            for name in output_devices()? { println!("{name}"); }
        }
        Some("play") if (2..=4).contains(&args.len()) => {
            let volume = args.get(2).map(|s| s.parse::<f32>()).transpose()?.unwrap_or(0.5);
            let sample = AudioSample::load(Path::new(&args[1]))?;
            println!("{} Hz, {} channels. Press Enter to stop.", sample.sample_rate, sample.channels);
            let playback = Playback::start(sample, args.get(3).map(String::as_str), volume)?;
            wait_for_stop(playback)?;
        }
        Some("pattern") if (5..=8).contains(&args.len()) => {
            let bpm = args.get(5).map(|s| s.parse::<f64>()).transpose()?.unwrap_or(120.0);
            let volume = args.get(6).map(|s| s.parse::<f32>()).transpose()?.unwrap_or(0.5);
            let samples = [
                AudioSample::load(Path::new(&args[1]))?,
                AudioSample::load(Path::new(&args[2]))?,
                AudioSample::load(Path::new(&args[3]))?,
                AudioSample::load(Path::new(&args[4]))?,
            ];
            let mut pattern = Pattern::default();
            for step in (0..STEPS).step_by(4) { pattern.set(0, step, 127)?; }
            for step in [4, 12] { pattern.set(1, step, 110)?; }
            for step in (0..STEPS).step_by(2) { pattern.set(2, step, 80)?; }
            pattern.set(3, 14, 100)?;
            let playback = Playback::start_pattern(samples, pattern, bpm, args.get(7).map(String::as_str), volume)?;
            println!("Four-track demo pattern at {bpm} BPM. Press Enter to stop.");
            wait_for_stop(playback)?;
        }
        Some("--help") | None => println!("Usage:\n  oxide-app devices\n  oxide-app play <file.wav> [volume: 0..1] [device name]\n  oxide-app pattern <kick.wav> <snare.wav> <hat.wav> <perc.wav> [bpm] [volume: 0..1] [device name]\nPattern loops a preset 16-step beat. Default BPM: 120.\nplay runs once, then outputs silence. pattern loops. Enter stops either. Default volume: 0.5."),
        _ => return Err("Invalid arguments. Run with --help".into()),
    }
    Ok(())
}

fn wait_for_stop(playback: Playback) -> Result<(), Box<dyn std::error::Error>> {
    // Keep stream ownership on this thread while input waits elsewhere.
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = io::stdin().read_line(&mut String::new());
        let _ = sender.send(result);
    });
    loop {
        if playback.failed.load(Ordering::Relaxed) {
            return Err("Audio stream failed (device disconnected or backend error)".into());
        }
        match receiver.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(result) => {
                result?;
                break;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
