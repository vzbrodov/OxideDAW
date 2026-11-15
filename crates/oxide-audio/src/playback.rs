//! Minimal playback with predecoded audio; callback performs no allocation or I/O.
use crate::pattern::PatternRenderer;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use oxide_core::sequencer::{Pattern, TRACKS};
use oxide_dsp::sample::AudioSample;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

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
}

impl Playback {
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
                Ok(Source::Pattern(Box::new(renderer)))
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
        let failed = Arc::new(AtomicBool::new(false));
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => build::<f32>(&device, &config, source, failed.clone())?,
            cpal::SampleFormat::I16 => build::<i16>(&device, &config, source, failed.clone())?,
            cpal::SampleFormat::U16 => build::<u16>(&device, &config, source, failed.clone())?,
            format => return Err(format!("Unsupported device sample format: {format}").into()),
        };
        stream.play()?;
        Ok(Self {
            _stream: stream,
            failed,
        })
    }
}

fn build<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut source: Source,
    failed: Arc<AtomicBool>,
) -> Result<cpal::Stream> {
    let channels = config.channels as usize;
    Ok(device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            // Bounded stack scratch, independent of backend callback size.
            let mut scratch = [[0.0; 2]; 256];
            for block in data.chunks_mut(channels * scratch.len()) {
                let frames = block.len() / channels;
                source.render(&mut scratch[..frames]);
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
        },
        move |_| {
            failed.store(true, Ordering::Relaxed);
        },
        None,
    )?)
}

enum Source {
    Sample(Renderer),
    Pattern(Box<PatternRenderer>),
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
            Self::Pattern(renderer) => renderer.render(output),
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
