//! CLI audio prototype. All decoding happens before starting the stream.
use oxide_audio::playback::{output_devices, Playback};
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
            // Keep stream ownership on this thread while input waits elsewhere.
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let result = io::stdin().read_line(&mut String::new());
                let _ = sender.send(result);
            });
            loop {
                if playback.failed.load(Ordering::Relaxed) { return Err("Audio stream failed (device disconnected or backend error)".into()); }
                match receiver.recv_timeout(std::time::Duration::from_millis(50)) {
                    Ok(result) => { result?; break; }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        Some("--help") | None => println!("Usage:\n  oxide-app devices\n  oxide-app play <file.wav> [volume: 0..1] [device name]\nAudio plays once, then outputs silence until Enter. Default volume: 0.5."),
        _ => return Err("Invalid arguments. Run with --help".into()),
    }
    Ok(())
}
