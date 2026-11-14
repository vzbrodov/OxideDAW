//! Oxide MIDI - MIDI events, clip storage, Piano Roll models.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub pitch: u8,
    pub velocity: u8,
    pub start_tick: u64,
    pub duration_ticks: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MidiClip {
    pub name: String,
    pub notes: Vec<Note>,
}
