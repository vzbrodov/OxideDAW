//! Fixed-size 4/4 step sequencer. Scheduling uses absolute sample positions so
//! rounding never accumulates from one step or loop to the next.
use serde::{Deserialize, Serialize};
pub const TRACKS: usize = 4;
pub const STEPS: usize = 16;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PatternData")]
pub struct Pattern {
    /// Zero means no trigger; other values are MIDI-style velocities (1..=127).
    steps: [[u8; STEPS]; TRACKS],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PatternData {
    steps: [[u8; STEPS]; TRACKS],
}

impl TryFrom<PatternData> for Pattern {
    type Error = &'static str;

    fn try_from(data: PatternData) -> Result<Self, Self::Error> {
        if data.steps.iter().flatten().any(|&velocity| velocity > 127) {
            return Err("Pattern velocities must be between 0 and 127");
        }
        Ok(Self { steps: data.steps })
    }
}

impl Pattern {
    pub fn get(&self, track: usize, step: usize) -> Option<u8> {
        self.steps
            .get(track)
            .and_then(|steps| steps.get(step))
            .copied()
    }

    pub fn drum_demo() -> Self {
        let mut pattern = Self::default();
        for step in (0..STEPS).step_by(4) {
            pattern.steps[0][step] = 127;
        }
        for step in [4, 12] {
            pattern.steps[1][step] = 110;
        }
        for step in (0..STEPS).step_by(2) {
            pattern.steps[2][step] = 80;
        }
        pattern.steps[3][14] = 100;
        pattern
    }

    pub fn set(&mut self, track: usize, step: usize, velocity: u8) -> Result<(), &'static str> {
        if track >= TRACKS || step >= STEPS || velocity > 127 {
            return Err("Invalid track, step or velocity");
        }
        self.steps[track][step] = velocity;
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Trigger {
    pub track: usize,
    pub step: usize,
    pub velocity: u8,
    pub offset: usize,
}

pub struct Transport {
    sample_rate: u32,
    bpm: f64,
    position: u64,
    next_step: u64,
    playing: bool,
    looping: bool,
}

impl Transport {
    pub fn new(sample_rate: u32, bpm: f64) -> Result<Self, &'static str> {
        if sample_rate == 0 || !bpm.is_finite() || !(1.0..=999.0).contains(&bpm) {
            return Err("Expected a positive sample rate and BPM between 1 and 999");
        }
        // A step must occupy at least one frame.
        if sample_rate as f64 * 15.0 / bpm < 1.0 {
            return Err("Sample rate too low for this tempo");
        }
        Ok(Self {
            sample_rate,
            bpm,
            position: 0,
            next_step: 0,
            playing: false,
            looping: true,
        })
    }

    pub fn play(&mut self) {
        self.playing = true;
    }
    pub fn pause(&mut self) {
        self.playing = false;
    }
    pub fn stop(&mut self) {
        self.playing = false;
        self.position = 0;
        self.next_step = 0;
    }
    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }
    pub fn position(&self) -> u64 {
        self.position
    }
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    fn step_frame(&self, step: u64) -> u64 {
        (step as f64 * self.sample_rate as f64 * 15.0 / self.bpm).round() as u64
    }

    /// Emit events for [position, position + frames). Caller supplies the sink;
    /// scheduling itself neither allocates nor locks. Stop resets; pause resumes.
    pub fn process(&mut self, frames: usize, pattern: &Pattern, mut emit: impl FnMut(Trigger)) {
        if !self.playing || frames == 0 {
            return;
        }
        let end = self.position.saturating_add(frames as u64);
        while self.looping || self.next_step < STEPS as u64 {
            let frame = self.step_frame(self.next_step);
            if frame >= end {
                break;
            }
            let step = self.next_step as usize % STEPS;
            for (track, steps) in pattern.steps.iter().enumerate() {
                let velocity = steps[step];
                if velocity != 0 {
                    emit(Trigger {
                        track,
                        step,
                        velocity,
                        offset: (frame - self.position) as usize,
                    });
                }
            }
            self.next_step += 1;
        }
        self.position = end;
        if !self.looping && end >= self.step_frame(STEPS as u64) {
            self.position = self.step_frame(STEPS as u64);
            self.playing = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pattern() -> Pattern {
        let mut pattern = Pattern::default();
        for track in 0..TRACKS {
            for step in 0..STEPS {
                pattern.set(track, step, 100).unwrap();
            }
        }
        pattern
    }
    fn render(chunks: &[usize], bpm: f64) -> Vec<(u64, usize, usize)> {
        let mut clock = Transport::new(48000, bpm).unwrap();
        clock.play();
        let pattern = pattern();
        let mut result = Vec::new();
        for &frames in chunks {
            let start = clock.position();
            clock.process(frames, &pattern, |event| {
                result.push((start + event.offset as u64, event.track, event.step))
            });
        }
        result
    }
    #[test]
    fn block_boundaries_do_not_change_event_positions() {
        assert_eq!(
            render(&[24000], 120.0),
            render(&[6000, 1, 5999, 12000], 120.0)
        );
        let events = render(&[6000, 1], 120.0);
        assert_eq!(events.len(), 8);
        assert_eq!(events[4], (6000, 0, 1));
    }
    #[test]
    fn fractional_tempo_has_no_accumulated_loop_rounding() {
        let events = render(&[1_000_000], 137.0);
        for &(frame, _, step) in &events {
            let absolute_step = (frame as f64 * 137.0 / (48000.0 * 15.0)).round() as u64;
            assert_eq!(
                frame,
                (absolute_step as f64 * 48000.0 * 15.0 / 137.0).round() as u64
            );
            assert_eq!(step, absolute_step as usize % STEPS);
        }
        assert_eq!(events, render(&vec![1000; 1000], 137.0));
    }
    #[test]
    fn stop_restarts_and_pause_preserves_position() {
        let mut clock = Transport::new(48000, 120.0).unwrap();
        clock.play();
        clock.process(100, &pattern(), |_| {});
        clock.pause();
        clock.process(100, &pattern(), |_| panic!("paused"));
        assert_eq!(clock.position(), 100);
        clock.play();
        clock.process(100, &pattern(), |_| panic!("duplicate start"));
        clock.stop();
        assert_eq!(clock.position(), 0);
        clock.play();
        let mut count = 0;
        clock.process(1, &pattern(), |_| count += 1);
        assert_eq!(count, 4);
    }
    #[test]
    fn one_shot_stops_at_bar_end() {
        let mut clock = Transport::new(48000, 120.0).unwrap();
        clock.set_looping(false);
        clock.play();
        let mut count = 0;
        clock.process(200000, &pattern(), |_| count += 1);
        assert_eq!(count, 64);
        assert_eq!(clock.position(), 96000);
        assert!(!clock.is_playing());
    }
    #[test]
    fn rejects_invalid_configuration() {
        for bpm in [0.0, f64::NAN, f64::INFINITY, 1000.0] {
            assert!(Transport::new(48000, bpm).is_err());
        }
        assert!(Transport::new(0, 120.0).is_err());
        assert!(Pattern::default().set(4, 0, 127).is_err());
        assert!(Pattern::default().set(0, 16, 127).is_err());
        assert!(Pattern::default().set(0, 0, 128).is_err());
    }
}
