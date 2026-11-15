//! A voice owns only playback state. Sample storage stays with the caller.
use crate::sample::AudioSample;

#[derive(Clone, Copy, Default)]
pub struct SamplerVoice {
    position: f64,
    gain: f32,
    active: bool,
}

impl SamplerVoice {
    pub fn trigger(&mut self, velocity: u8) {
        self.position = 0.0;
        self.gain = velocity.min(127) as f32 / 127.0;
        self.active = velocity != 0;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Linear interpolation; samples and output rate must be validated up front.
    /// No clipping here: the mixer limits the sum once all voices are added.
    pub fn next_frame(&mut self, sample: &AudioSample, output_rate: u32) -> [f32; 2] {
        let frames = sample.samples.len() / sample.channels;
        let index = self.position as usize;
        if !self.active || index >= frames {
            self.active = false;
            return [0.0; 2];
        }
        let next = (index + 1).min(frames - 1);
        let fraction = (self.position - index as f64) as f32;
        let output = std::array::from_fn(|channel| {
            let channel = channel.min(sample.channels - 1);
            let a = sample.samples[index * sample.channels + channel];
            let b = sample.samples[next * sample.channels + channel];
            let value = (a + (b - a) * fraction) * self.gain;
            if value.is_finite() {
                value
            } else {
                0.0
            }
        });
        self.position += sample.sample_rate as f64 / output_rate as f64;
        if self.position >= frames as f64 {
            self.active = false;
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resamples_mono_and_silences_after_end() {
        let sample = AudioSample {
            samples: vec![0.0, 1.0],
            channels: 1,
            sample_rate: 2,
        };
        let mut voice = SamplerVoice::default();
        voice.trigger(127);
        for expected in [0.0, 0.5, 1.0, 1.0, 0.0] {
            assert_eq!(voice.next_frame(&sample, 4), [expected; 2]);
        }
        assert!(!voice.is_active());
    }
}
