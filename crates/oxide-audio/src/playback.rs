//! Minimal playback with predecoded audio; callback performs no allocation or I/O.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
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
        let failed = Arc::new(AtomicBool::new(false));
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => {
                build::<f32>(&device, &config, sample, volume, failed.clone())?
            }
            cpal::SampleFormat::I16 => {
                build::<i16>(&device, &config, sample, volume, failed.clone())?
            }
            cpal::SampleFormat::U16 => {
                build::<u16>(&device, &config, sample, volume, failed.clone())?
            }
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
    sample: AudioSample,
    volume: f32,
    failed: Arc<AtomicBool>,
) -> Result<cpal::Stream> {
    let channels = config.channels as usize;
    let mut renderer = Renderer {
        sample,
        position: 0.0,
        rate: config.sample_rate.0,
        volume,
    };
    Ok(device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            for frame in data.chunks_mut(channels) {
                let (left, right) = renderer.next_frame();
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
        },
        move |_| {
            failed.store(true, Ordering::Relaxed);
        },
        None,
    )?)
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
