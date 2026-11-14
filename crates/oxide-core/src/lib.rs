//! Oxide Core - Central data models, timeline, project definitions.
pub mod sequencer;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub bpm: f32,
    pub sample_rate: u32,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            name: String::from("Untitled Project"),
            bpm: 140.0,
            sample_rate: 44100,
        }
    }
}
