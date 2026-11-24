//! Minimal playback with predecoded audio; callback performs no allocation or I/O.
use crate::pattern::PatternRenderer;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use oxide_core::sequencer::{sequence_step_count, Pattern, PlaybackBar, TRACKS};
use oxide_dsp::sample::AudioSample;
use ringbuf::{
    traits::{Consumer, Producer, Split},
    HeapCons, HeapProd, HeapRb,
};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc,
};

const COMMAND_CAPACITY: usize = 64;

/// Small, allocation-free messages applied in FIFO order at audio callback boundaries.
#[derive(Clone, Debug, PartialEq)]
pub enum PlaybackCommand {
    Play,
    Pause,
    Stop,
    SeekStep(usize),
    SetBpm(f64),
    SetState {
        pattern: Pattern,
        bpm: f64,
        volume: f32,
    },
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn output_devices() -> Result<Vec<String>> {
    cpal::default_host()
        .output_devices()?
        .map(|device| Ok(device.name()?))
        .collect()
}

pub struct Playback {
    _stream: cpal::Stream,
    pub failed: Arc<AtomicBool>,
    commands: HeapProd<PlaybackCommand>,
    sample_rate: u32,
    step_count: usize,
    position_steps: Arc<AtomicU32>,
    peaks: Arc<[AtomicU32; TRACKS + 2]>,
}

impl Playback {
    /// Queue a sequencer control without blocking. A full queue rejects the new command;
    /// callers can retry later. Successful enqueue does not mean it was rendered yet.
    pub fn command(&mut self, command: PlaybackCommand) -> Result<()> {
        if self.step_count == 0 {
            return Err("Transport commands require sequencer playback".into());
        }
        match &command {
            PlaybackCommand::SetState { bpm, volume, .. } => {
                oxide_core::sequencer::Transport::new(self.sample_rate, *bpm)?;
                if !volume.is_finite() || !(0.0..=1.0).contains(volume) {
                    return Err("Volume must be between 0 and 1".into());
                }
            }
            PlaybackCommand::SetBpm(bpm) => {
                oxide_core::sequencer::Transport::new(self.sample_rate, *bpm)?;
            }
            PlaybackCommand::SeekStep(step) if *step >= self.step_count => {
                return Err(format!("Expected step between 0 and {}", self.step_count - 1).into());
            }
            _ => {}
        }
        self.commands
            .try_push(command)
            .map_err(|_| "Playback command queue is full; retry later".into())
    }

    /// Musical position reported by the audio callback, in continuous steps.
    pub fn position_steps(&self) -> f32 {
        f32::from_bits(self.position_steps.load(Ordering::Relaxed))
    }

    /// Highest per-channel and output peaks since the previous UI read.
    pub fn take_peaks(&self) -> [f32; TRACKS + 2] {
        std::array::from_fn(|i| f32::from_bits(self.peaks[i].swap(0, Ordering::Relaxed)))
    }

    pub fn start(sample: AudioSample, device_name: Option<&str>, volume: f32) -> Result<Self> {
        sample.validate()?;
        Self::start_source(
            device_name,
            move |rate| {
                Ok(Source::Sample(Renderer {
                    sample,
                    position: 0.0,
                    rate,
                    volume,
                }))
            },
            volume,
        )
    }

    pub fn start_pattern(
        samples: [AudioSample; TRACKS],
        pattern: Pattern,
        bpm: f64,
        device_name: Option<&str>,
        volume: f32,
    ) -> Result<Self> {
        Self::start_arrangement(samples, pattern, Vec::new(), bpm, device_name, volume)
    }

    /// Loop a fixed prepared arrangement. An empty list loops the main pattern.
    pub fn start_arrangement(
        samples: [AudioSample; TRACKS],
        pattern: Pattern,
        arrangement: Vec<PlaybackBar>,
        bpm: f64,
        device_name: Option<&str>,
        volume: f32,
    ) -> Result<Self> {
        for sample in &samples {
            sample.validate()?;
        }
        // Validate tempo before device enumeration.
        oxide_core::sequencer::Transport::new(48000, bpm)?;
        Self::start_source(
            device_name,
            move |rate| {
                let mut renderer = PatternRenderer::new(samples, pattern, rate, bpm, volume)?;
                renderer.play();
                Ok(Source::Pattern {
                    renderer: Box::new(renderer),
                    arrangement,
                })
            },
            volume,
        )
    }

    fn start_source(
        device_name: Option<&str>,
        create: impl FnOnce(u32) -> Result<Source>,
        volume: f32,
    ) -> Result<Self> {
        if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
            return Err("Volume must be between 0 and 1".into());
        }
        let host = cpal::default_host();
        let device = if let Some(name) = device_name {
            host.output_devices()?
                .find(|d| d.name().ok().as_deref() == Some(name))
        } else {
            host.default_output_device()
        }
        .ok_or("Output device not found")?;
        let supported = device.default_output_config()?;
        let config = supported.config();
        if config.channels == 0 || config.sample_rate.0 == 0 {
            return Err("Invalid device configuration".into());
        }
        let source = create(config.sample_rate.0)?;
        let step_count = match &source {
            Source::Pattern { arrangement, .. } => sequence_step_count(arrangement),
            Source::Sample(_) => 0,
        };
        let (commands, consumer) = HeapRb::new(COMMAND_CAPACITY).split();
        let failed = Arc::new(AtomicBool::new(false));
        let position_steps = Arc::new(AtomicU32::new(0.0_f32.to_bits()));
        let peaks = Arc::new(std::array::from_fn(|_| AtomicU32::new(0)));
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => build::<f32>(
                &device,
                &config,
                source,
                consumer,
                failed.clone(),
                position_steps.clone(),
                peaks.clone(),
            )?,
            cpal::SampleFormat::I16 => build::<i16>(
                &device,
                &config,
                source,
                consumer,
                failed.clone(),
                position_steps.clone(),
                peaks.clone(),
            )?,
            cpal::SampleFormat::U16 => build::<u16>(
                &device,
                &config,
                source,
                consumer,
                failed.clone(),
                position_steps.clone(),
                peaks.clone(),
            )?,
            format => return Err(format!("Unsupported device sample format: {format}").into()),
        };
        stream.play()?;
        Ok(Self {
            _stream: stream,
            failed,
            commands,
            sample_rate: config.sample_rate.0,
            step_count,
            position_steps,
            peaks,
        })
    }
}

fn build<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut source: Source,
    mut commands: HeapCons<PlaybackCommand>,
    failed: Arc<AtomicBool>,
    position_steps: Arc<AtomicU32>,
    peaks: Arc<[AtomicU32; TRACKS + 2]>,
) -> Result<cpal::Stream> {
    let channels = config.channels as usize;
    Ok(device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            apply_commands(&mut source, &mut commands);
            // Bounded stack scratch, independent of backend callback size.
            let mut scratch = [[0.0; 2]; 256];
            let mut callback_peaks = [0.0; TRACKS + 2];
            for block in data.chunks_mut(channels * scratch.len()) {
                let frames = block.len() / channels;
                source.render(&mut scratch[..frames]);
                collect_peaks(&source, &scratch[..frames], &mut callback_peaks);
                for (frame, &[left, right]) in
                    block.chunks_exact_mut(channels).zip(&scratch[..frames])
                {
                    for (channel, output) in frame.iter_mut().enumerate() {
                        let value = if channels == 1 {
                            (left + right) * 0.5
                        } else {
                            match channel {
                                0 => left,
                                1 => right,
                                _ => 0.0,
                            }
                        };
                        *output = T::from_sample(value);
                    }
                }
            }
            if let Source::Pattern { renderer, .. } = &source {
                position_steps.store(
                    (renderer.position_steps() as f32).to_bits(),
                    Ordering::Relaxed,
                );
            }
            for (meter, value) in peaks.iter().zip(callback_peaks) {
                meter.fetch_max(value.to_bits(), Ordering::Relaxed);
            }
        },
        move |_| {
            failed.store(true, Ordering::Relaxed);
        },
        None,
    )?)
}

fn collect_peaks(source: &Source, frames: &[[f32; 2]], peaks: &mut [f32; TRACKS + 2]) {
    match source {
        Source::Pattern { renderer, .. } => {
            for (peak, value) in peaks.iter_mut().zip(renderer.peaks()) {
                *peak = peak.max(value);
            }
        }
        Source::Sample(_) => {
            for frame in frames {
                peaks[TRACKS] = peaks[TRACKS].max(frame[0].abs());
                peaks[TRACKS + 1] = peaks[TRACKS + 1].max(frame[1].abs());
            }
        }
    }
}

fn apply_commands(source: &mut Source, commands: &mut HeapCons<PlaybackCommand>) {
    // Bound work even if the producer keeps adding commands during this callback.
    for _ in 0..COMMAND_CAPACITY {
        let Some(command) = commands.try_pop() else {
            break;
        };
        if let Source::Pattern {
            renderer,
            arrangement,
        } = source
        {
            match command {
                PlaybackCommand::Play => renderer.play(),
                PlaybackCommand::Pause => renderer.pause(),
                PlaybackCommand::Stop => renderer.stop(),
                PlaybackCommand::SeekStep(step) => {
                    let _ = renderer.seek_sequence_step(step, arrangement);
                }
                PlaybackCommand::SetBpm(bpm) => {
                    let _ = renderer.set_bpm(bpm);
                }
                PlaybackCommand::SetState {
                    pattern,
                    bpm,
                    volume,
                } => {
                    let _ = renderer.set_state(pattern, bpm, volume);
                }
            }
        }
    }
}

enum Source {
    Sample(Renderer),
    Pattern {
        renderer: Box<PatternRenderer>,
        arrangement: Vec<PlaybackBar>,
    },
}

impl Source {
    fn render(&mut self, output: &mut [[f32; 2]]) {
        match self {
            Self::Sample(renderer) => {
                for frame in output {
                    let (left, right) = renderer.next_frame();
                    *frame = [left, right];
                }
            }
            Self::Pattern {
                renderer,
                arrangement,
            } => renderer.render_sequence(output, arrangement),
        }
    }
}

struct Renderer {
    sample: AudioSample,
    position: f64,
    rate: u32,
    volume: f32,
}
impl Renderer {
    fn next_frame(&mut self) -> (f32, f32) {
        let frames = self.sample.samples.len() / self.sample.channels;
        let index = self.position as usize;
        if index >= frames {
            return (0.0, 0.0);
        }
        let next = (index + 1).min(frames - 1);
        let fraction = (self.position - index as f64) as f32;
        let at = |channel: usize| {
            let channel = channel.min(self.sample.channels - 1);
            let a = self.sample.samples[index * self.sample.channels + channel];
            let b = self.sample.samples[next * self.sample.channels + channel];
            let value = (a + (b - a) * fraction) * self.volume;
            if value.is_finite() {
                value.clamp(-1.0, 1.0)
            } else {
                0.0
            }
        };
        let output = (at(0), at(1));
        self.position += self.sample.sample_rate as f64 / self.rate as f64;
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_meters_keep_early_chunk_peaks_and_measure_audition_stereo() {
        let samples = std::array::from_fn(|_| AudioSample {
            samples: vec![0.5],
            channels: 1,
            sample_rate: 8,
        });
        let mut pattern = Pattern::default();
        pattern.set(0, 0, 127).unwrap();
        let mut renderer = PatternRenderer::new(samples, pattern, 8, 120.0, 0.5).unwrap();
        renderer.set_looping(false);
        renderer.play();
        let mut source = Source::Pattern {
            renderer: Box::new(renderer),
            arrangement: Vec::new(),
        };
        let mut scratch = [[0.0; 2]; 256];
        let mut peaks = [0.0; TRACKS + 2];
        source.render(&mut scratch);
        collect_peaks(&source, &scratch, &mut peaks);
        source.render(&mut scratch);
        collect_peaks(&source, &scratch, &mut peaks);
        assert_eq!(peaks, [0.5, 0.0, 0.0, 0.0, 0.25, 0.25]);
        let mut source = Source::Sample(Renderer {
            sample: AudioSample {
                samples: vec![0.5, -1.0],
                channels: 2,
                sample_rate: 8,
            },
            position: 0.0,
            rate: 8,
            volume: 0.5,
        });
        let mut peaks = [0.0; TRACKS + 2];
        source.render(&mut scratch);
        collect_peaks(&source, &scratch, &mut peaks);
        assert_eq!(peaks, [0.0, 0.0, 0.0, 0.0, 0.25, 0.5]);
    }

    #[test]
    fn queued_controls_are_fifo_and_overflow_preserves_existing_commands() {
        let samples = std::array::from_fn(|_| AudioSample {
            samples: vec![0.25; 64],
            channels: 1,
            sample_rate: 8,
        });
        let mut pattern = Pattern::default();
        pattern.set(0, 4, 127).unwrap();
        let renderer = PatternRenderer::new(samples, pattern, 8, 120.0, 1.0).unwrap();
        let mut source = Source::Pattern {
            renderer: Box::new(renderer),
            arrangement: Vec::new(),
        };
        let (mut producer, mut consumer) = HeapRb::new(COMMAND_CAPACITY).split();
        for command in [
            PlaybackCommand::Stop,
            PlaybackCommand::SetBpm(60.0),
            PlaybackCommand::SeekStep(4),
            PlaybackCommand::Play,
        ] {
            producer.try_push(command).unwrap();
        }
        apply_commands(&mut source, &mut consumer);
        let mut output = [[0.0; 2]; 1];
        source.render(&mut output);
        assert_eq!(output[0], [0.25; 2]);
        for _ in 0..COMMAND_CAPACITY {
            producer.try_push(PlaybackCommand::Pause).unwrap();
        }
        assert_eq!(
            producer.try_push(PlaybackCommand::Play),
            Err(PlaybackCommand::Play)
        );
        apply_commands(&mut source, &mut consumer);
        source.render(&mut output);
        assert_eq!(output[0], [0.0; 2]);
        producer.try_push(PlaybackCommand::Play).unwrap();
        apply_commands(&mut source, &mut consumer);
        source.render(&mut output);
        assert_eq!(output[0], [0.25; 2]);
    }

    #[test]
    fn arrangement_controls_seek_across_bars_and_loop_without_resetting_voices() {
        // At 8 Hz and 120 BPM, each step occupies one frame.
        let samples = std::array::from_fn(|track| AudioSample {
            samples: vec![if track == 0 { 0.25 } else { 0.5 }; 4],
            channels: 1,
            sample_rate: 8,
        });
        let mut main = Pattern::default();
        main.set(0, 0, 127).unwrap();
        main.set(0, 15, 127).unwrap();
        let mut variation = Pattern::default();
        variation.set(1, 0, 127).unwrap();
        variation.set(1, 15, 127).unwrap();
        let arrangement = vec![
            PlaybackBar::Main {},
            PlaybackBar::Rest {},
            PlaybackBar::Variation { pattern: variation },
        ];
        let mut renderer = PatternRenderer::new(samples, main, 8, 120.0, 1.0).unwrap();
        renderer.play();
        let mut source = Source::Pattern {
            renderer: Box::new(renderer),
            arrangement,
        };
        let (mut producer, mut consumer) = HeapRb::new(COMMAND_CAPACITY).split();
        let mut command = |source: &mut Source, command| {
            producer.try_push(command).unwrap();
            apply_commands(source, &mut consumer);
        };
        let mut bar = [[0.0; 2]; 16];
        source.render(&mut bar);
        assert_eq!(bar[15], [0.25; 2]);

        let mut frame = [[0.0; 2]; 1];
        command(&mut source, PlaybackCommand::Pause);
        source.render(&mut frame);
        assert_eq!(frame[0], [0.0; 2]);
        command(&mut source, PlaybackCommand::Play);
        source.render(&mut frame);
        assert_eq!(frame[0], [0.25; 2]); // The preceding hit carries into the rest.

        command(&mut source, PlaybackCommand::SeekStep(32));
        source.render(&mut frame);
        assert_eq!(frame[0], [0.5; 2]); // The seek cuts the old voice.
        source.render(&mut bar);
        assert_eq!(bar[15], [0.75; 2]); // The variation tail overlaps the first hit.

        command(&mut source, PlaybackCommand::Stop);
        source.render(&mut frame);
        assert_eq!(frame[0], [0.0; 2]);
        command(&mut source, PlaybackCommand::Play);
        source.render(&mut frame);
        assert_eq!(frame[0], [0.25; 2]);
    }

    #[test]
    fn resamples_mono_and_silences_after_end() {
        let mut renderer = Renderer {
            sample: AudioSample {
                samples: vec![0.0, 1.0],
                channels: 1,
                sample_rate: 2,
            },
            position: 0.0,
            rate: 4,
            volume: 0.5,
        };
        assert_eq!(renderer.next_frame(), (0.0, 0.0));
        assert_eq!(renderer.next_frame(), (0.25, 0.25));
        assert_eq!(renderer.next_frame(), (0.5, 0.5));
        assert_eq!(renderer.next_frame(), (0.5, 0.5));
        assert_eq!(renderer.next_frame(), (0.0, 0.0));
    }
}
