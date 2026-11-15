//! Four-track offline/realtime renderer with bounded polyphony and no render-time allocation.
use oxide_core::sequencer::{Pattern, Transport, TRACKS};
use oxide_dsp::{sample::AudioSample, SamplerVoice};

pub const VOICES_PER_TRACK: usize = 8;

struct Track {
    sample: AudioSample,
    voices: [SamplerVoice; VOICES_PER_TRACK],
    next_voice: usize,
}

impl Track {
    fn trigger(&mut self, velocity: u8) {
        // Round-robin stealing replaces the oldest trigger when the pool is full.
        self.voices[self.next_voice].trigger(velocity);
        self.next_voice = (self.next_voice + 1) % VOICES_PER_TRACK;
    }

    fn mix(&mut self, rate: u32) -> [f32; 2] {
        let mut sum = [0.0; 2];
        for voice in &mut self.voices {
            let frame = voice.next_frame(&self.sample, rate);
            sum[0] += frame[0];
            sum[1] += frame[1];
        }
        sum
    }
}

pub struct PatternRenderer {
    pattern: Pattern,
    transport: Transport,
    tracks: [Track; TRACKS],
    sample_rate: u32,
    volume: f32,
    paused: bool,
}

impl PatternRenderer {
    /// Samples, voices and pattern are prepared before entering the audio callback.
    pub fn new(
        samples: [AudioSample; TRACKS],
        pattern: Pattern,
        sample_rate: u32,
        bpm: f64,
        volume: f32,
    ) -> Result<Self, &'static str> {
        if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
            return Err("Volume must be between 0 and 1");
        }
        for sample in &samples {
            sample.validate()?;
        }
        let transport = Transport::new(sample_rate, bpm)?;
        Ok(Self {
            pattern,
            transport,
            tracks: samples.map(|sample| Track {
                sample,
                voices: [SamplerVoice::default(); VOICES_PER_TRACK],
                next_voice: 0,
            }),
            sample_rate,
            volume,
            paused: false,
        })
    }

    pub fn play(&mut self) {
        self.paused = false;
        self.transport.play();
    }
    /// Pause freezes both the clock and sample tails.
    pub fn pause(&mut self) {
        self.paused = true;
        self.transport.pause();
    }
    pub fn stop(&mut self) {
        self.transport.stop();
        self.paused = false;
        for track in &mut self.tracks {
            track.voices.fill(SamplerVoice::default());
            track.next_voice = 0;
        }
    }
    pub fn set_looping(&mut self, looping: bool) {
        self.transport.set_looping(looping);
    }

    /// Render stereo frames, placing triggers at their exact offsets in this block.
    /// One-shot playback drains sample tails after the final step.
    pub fn render(&mut self, output: &mut [[f32; 2]]) {
        output.fill([0.0; 2]);
        let draining = !self.transport.is_playing() && self.transport.position() != 0;
        if !self.transport.is_playing() && !draining {
            return;
        }
        // A paused transport must preserve voices; one-shot completion may drain them.
        if self.paused {
            return;
        }
        let mut cursor = 0;
        let tracks = &mut self.tracks;
        let rate = self.sample_rate;
        let volume = self.volume;
        self.transport
            .process(output.len(), &self.pattern, |event| {
                mix(tracks, rate, volume, &mut output[cursor..event.offset]);
                cursor = event.offset;
                tracks[event.track].trigger(event.velocity);
            });
        mix(tracks, rate, volume, &mut output[cursor..]);
    }
}

fn mix(tracks: &mut [Track; TRACKS], rate: u32, volume: f32, output: &mut [[f32; 2]]) {
    for frame in output {
        let mut sum = [0.0; 2];
        for track in tracks.iter_mut() {
            let value = track.mix(rate);
            sum[0] += value[0];
            sum[1] += value[1];
        }
        *frame = sum.map(|value| (value * volume).clamp(-1.0, 1.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(values: &[f32], channels: usize) -> [AudioSample; TRACKS] {
        std::array::from_fn(|_| AudioSample {
            samples: values.to_vec(),
            channels,
            sample_rate: 8,
        })
    }

    fn renderer(values: &[f32], pattern: Pattern) -> PatternRenderer {
        let mut renderer =
            PatternRenderer::new(samples(values, 1), pattern, 8, 120.0, 1.0).unwrap();
        renderer.play();
        renderer
    }

    #[test]
    fn audible_impulses_match_across_blocks_and_loop_boundary() {
        // At 8 Hz / 120 BPM each step is one frame and a bar is 16 frames.
        let mut pattern = Pattern::default();
        for (track, step) in [(0, 0), (1, 3), (2, 7), (3, 15)] {
            pattern.set(track, step, 127).unwrap();
        }
        let mut whole = renderer(&[0.25], pattern.clone());
        let mut split = renderer(&[0.25], pattern);
        let mut expected = [[0.0; 2]; 33];
        whole.render(&mut expected);
        for (index, frame) in expected.iter().enumerate() {
            assert_eq!(
                *frame,
                [if [0, 3, 7, 15].contains(&(index % 16)) {
                    0.25
                } else {
                    0.0
                }; 2]
            );
        }
        let mut actual = [[0.0; 2]; 33];
        let mut start = 0;
        for size in [3, 1, 11, 1, 1, 16] {
            split.render(&mut actual[start..start + size]);
            start += size;
        }
        assert_eq!(actual, expected);
    }

    #[test]
    fn overlapping_voices_are_bounded_and_stop_clears_tails() {
        let mut pattern = Pattern::default();
        for step in 0..16 {
            pattern.set(0, step, 127).unwrap();
        }
        let mut renderer = renderer(&[0.0625; 64], pattern);
        let mut output = [[0.0; 2]; 20];
        renderer.render(&mut output);
        for (index, frame) in output.iter().enumerate() {
            assert_eq!(
                *frame,
                [((index + 1).min(VOICES_PER_TRACK) as f32) * 0.0625; 2]
            );
        }
        renderer.pause();
        renderer.render(&mut output);
        assert_eq!(output, [[0.0; 2]; 20]);
        renderer.play();
        renderer.render(&mut output[..1]);
        assert_eq!(output[0], [0.5; 2]);
        renderer.stop();
        renderer.render(&mut output);
        assert_eq!(output, [[0.0; 2]; 20]);
        renderer.play();
        renderer.render(&mut output[..1]);
        assert_eq!(output[0], [0.0625; 2]);
    }

    #[test]
    fn stereo_velocity_master_volume_and_final_limiting() {
        let mut pattern = Pattern::default();
        for track in 0..TRACKS {
            pattern.set(track, 0, 64).unwrap();
        }
        let mut renderer =
            PatternRenderer::new(samples(&[1.0, -0.5], 2), pattern, 8, 120.0, 0.75).unwrap();
        renderer.play();
        let mut output = [[0.0; 2]; 1];
        renderer.render(&mut output);
        assert_eq!(output[0][0], 1.0);
        assert!((output[0][1] - (-0.5 * 4.0 * 64.0 / 127.0 * 0.75)).abs() < 1e-6);
    }

    #[test]
    fn one_shot_drains_tail_without_retriggering() {
        let mut pattern = Pattern::default();
        pattern.set(0, 15, 127).unwrap();
        let mut renderer = renderer(&[0.5; 3], pattern);
        renderer.set_looping(false);
        let mut output = [[0.0; 2]; 16];
        renderer.render(&mut output);
        assert_eq!(output[15], [0.5; 2]);
        let mut tail = [[0.0; 2]; 32];
        renderer.render(&mut tail);
        assert_eq!(&tail[..2], &[[0.5; 2]; 2]);
        assert!(tail[2..].iter().all(|frame| *frame == [0.0; 2]));
    }

    #[test]
    fn rejects_invalid_samples_and_configuration() {
        for (channels, rate, values) in [
            (0, 8, vec![1.0]),
            (3, 8, vec![1.0; 3]),
            (2, 8, vec![1.0]),
            (1, 0, vec![1.0]),
            (1, 8, vec![]),
        ] {
            let invalid = std::array::from_fn(|_| AudioSample {
                samples: values.clone(),
                channels,
                sample_rate: rate,
            });
            assert!(PatternRenderer::new(invalid, Pattern::default(), 8, 120.0, 1.0).is_err());
        }
        for volume in [-1.0, 1.1, f32::NAN] {
            assert!(
                PatternRenderer::new(samples(&[0.5], 1), Pattern::default(), 8, 120.0, volume)
                    .is_err()
            );
        }
    }
}
