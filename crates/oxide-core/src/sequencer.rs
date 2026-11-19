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

/// Each arrangement entry occupies one full 16-step bar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArrangementBar {
    // Empty struct variants let Serde reject extra fields.
    Main {},
    Variation { pattern: Pattern },
    Rest {},
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
    anchor_step: f64,
    anchor_frame: u64,
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
            anchor_step: 0.0,
            anchor_frame: 0,
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
        self.anchor_step = 0.0;
        self.anchor_frame = 0;
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

    /// Change tempo without moving the playhead in musical time or retriggering hits.
    /// Already sounding samples keep their original playback rate.
    pub fn set_bpm(&mut self, bpm: f64) -> Result<(), &'static str> {
        Self::new(self.sample_rate, bpm)?;
        if bpm == self.bpm {
            return Ok(());
        }
        self.anchor_step = if self.step_frame(self.next_step) == self.position {
            self.next_step as f64
        } else {
            self.anchor_step
                + (self.position as f64 - self.anchor_frame as f64) * self.bpm
                    / (self.sample_rate as f64 * 15.0)
        };
        self.anchor_frame = self.position;
        self.bpm = bpm;
        Ok(())
    }

    /// Jump to a zero-indexed step in the current pattern, preserving play/pause.
    /// The destination triggers on the next nonempty block after playback resumes.
    pub fn seek_step(&mut self, step: usize) -> Result<(), &'static str> {
        if step >= STEPS {
            return Err("Step must be between 0 and 15");
        }
        self.position = (step as f64 * self.sample_rate as f64 * 15.0 / self.bpm).round() as u64;
        self.anchor_frame = self.position;
        self.anchor_step = step as f64;
        self.next_step = step as u64;
        Ok(())
    }

    /// Continuous musical position, including completed loops.
    pub fn position_steps(&self) -> f64 {
        self.anchor_step
            + (self.position as f64 - self.anchor_frame as f64) * self.bpm
                / (self.sample_rate as f64 * 15.0)
    }

    fn step_frame(&self, step: u64) -> u64 {
        (self.anchor_frame as f64
            + (step as f64 - self.anchor_step) * self.sample_rate as f64 * 15.0 / self.bpm)
            .round() as u64
    }

    /// Scheduled end of the first pattern, using the current tempo anchor.
    pub fn one_shot_end_frame(&self) -> u64 {
        self.sequence_end_frame(&[])
    }

    /// Scheduled end of a sequence. An empty arrangement plays one bar.
    pub fn sequence_end_frame(&self, bars: &[ArrangementBar]) -> u64 {
        self.step_frame(sequence_steps(bars))
    }

    /// Emit events for [position, position + frames). Caller supplies the sink;
    /// scheduling itself neither allocates nor locks. Stop resets; pause resumes.
    pub fn process(&mut self, frames: usize, pattern: &Pattern, emit: impl FnMut(Trigger)) {
        self.process_arrangement(frames, pattern, &[], emit);
    }

    /// Schedule a sequence of bars using the same absolute clock as a single pattern.
    /// The arrangement must stay fixed while the sequence is playing.
    pub fn process_arrangement(
        &mut self,
        frames: usize,
        main: &Pattern,
        bars: &[ArrangementBar],
        mut emit: impl FnMut(Trigger),
    ) {
        if !self.playing || frames == 0 {
            return;
        }
        let total_steps = sequence_steps(bars);
        let end = self.position.saturating_add(frames as u64);
        while self.looping || self.next_step < total_steps {
            let frame = self.step_frame(self.next_step);
            if frame >= end {
                break;
            }
            let sequence_step = self.next_step % total_steps;
            self.next_step += 1;
            let pattern = match bars.get((sequence_step / STEPS as u64) as usize) {
                Some(ArrangementBar::Rest {}) => continue,
                Some(ArrangementBar::Variation { pattern }) => pattern,
                Some(ArrangementBar::Main {}) | None => main,
            };
            let step = (sequence_step % STEPS as u64) as usize;
            for (track, steps) in pattern.steps.iter().enumerate() {
                let velocity = steps[step];
                if velocity != 0 {
                    emit(Trigger {
                        track,
                        step,
                        velocity,
                        offset: frame.saturating_sub(self.position) as usize,
                    });
                }
            }
        }
        self.position = end;
        if !self.looping {
            let end_frame = self.step_frame(total_steps);
            if end >= end_frame {
                self.position = end_frame;
                self.playing = false;
            }
        }
    }
}

fn sequence_steps(bars: &[ArrangementBar]) -> u64 {
    bars.len().max(1) as u64 * STEPS as u64
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
    fn tempo_change_preserves_phase_and_pending_boundary() {
        let mut clock = Transport::new(48000, 120.0).unwrap();
        clock.play();
        clock.process(3000, &pattern(), |_| {});
        clock.set_bpm(240.0).unwrap();
        assert_eq!(clock.position(), 3000);
        let mut events = Vec::new();
        clock.process(1501, &pattern(), |event| events.push(event));
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].step, 1);
        assert_eq!(events[0].offset, 1500);
        // Changing tempo exactly at a pending boundary emits it once at offset zero.
        clock.seek_step(4).unwrap();
        clock.set_bpm(90.0).unwrap();
        events.clear();
        clock.process(1, &pattern(), |event| events.push(event));
        assert_eq!(events.len(), 4);
        assert_eq!((events[0].step, events[0].offset), (4, 0));
        clock.set_bpm(120.0).unwrap();
        clock.process(1, &pattern(), |_| panic!("duplicate trigger"));
    }

    #[test]
    fn tempo_change_keeps_rounded_boundary_due_and_rejects_too_fast_tempo() {
        let mut clock = Transport::new(48000, 137.0).unwrap();
        clock.play();
        let boundary = clock.step_frame(1);
        clock.process(boundary as usize, &pattern(), |_| {});
        clock.set_bpm(1.0).unwrap();
        let mut events = Vec::new();
        clock.process(1, &pattern(), |event| events.push(event));
        assert_eq!(events.len(), 4);
        assert_eq!((events[0].step, events[0].offset), (1, 0));
        let mut slow = Transport::new(8, 120.0).unwrap();
        assert!(slow.set_bpm(121.0).is_err());
        slow.play();
        let mut steps = Vec::new();
        slow.process(2, &pattern(), |event| steps.push(event.step));
        assert_eq!(steps, vec![0, 0, 0, 0, 1, 1, 1, 1]);
    }

    #[test]
    fn changed_tempo_is_independent_of_block_sizes() {
        fn after_change(chunks: &[usize]) -> Vec<(u64, usize, usize)> {
            let mut clock = Transport::new(48000, 137.0).unwrap();
            clock.play();
            clock.process(12345, &pattern(), |_| {});
            clock.set_bpm(173.25).unwrap();
            let mut events = Vec::new();
            for &frames in chunks {
                let start = clock.position();
                clock.process(frames, &pattern(), |event| {
                    events.push((start + event.offset as u64, event.track, event.step));
                });
            }
            events
        }
        assert_eq!(after_change(&[1_000_000]), after_change(&[1000; 1000]));
    }

    #[test]
    fn one_shot_end_follows_live_tempo_change() {
        let mut clock = Transport::new(48000, 120.0).unwrap();
        clock.set_looping(false);
        clock.play();
        clock.process(45000, &pattern(), |_| {});
        clock.set_bpm(60.0).unwrap();
        let mut steps = Vec::new();
        clock.process(200000, &pattern(), |event| steps.push(event.step));
        assert_eq!(steps.len(), 32);
        assert!(steps.iter().all(|&step| (8..16).contains(&step)));
        assert_eq!(clock.position(), 147000);
        assert!(!clock.is_playing());
    }

    #[test]
    fn seek_preserves_pause_and_one_shot_end() {
        let mut clock = Transport::new(48000, 120.0).unwrap();
        clock.set_looping(false);
        clock.seek_step(15).unwrap();
        assert!(!clock.is_playing());
        clock.process(10, &pattern(), |_| panic!("stopped"));
        clock.play();
        let mut steps = Vec::new();
        clock.process(12000, &pattern(), |event| steps.push(event.step));
        assert_eq!(steps, vec![15; 4]);
        assert_eq!(clock.position(), 96000);
        assert!(!clock.is_playing());
        clock.seek_step(0).unwrap();
        clock.set_bpm(60.0).unwrap();
        clock.play();
        clock.process(1, &pattern(), |event| assert_eq!(event.step, 0));
        clock.pause();
        let before = clock.position();
        assert!(clock.seek_step(16).is_err());
        assert!(clock.set_bpm(f64::NAN).is_err());
        assert_eq!(clock.position(), before);
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
