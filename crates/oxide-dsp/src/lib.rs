//! Oxide DSP - Digital signal processing, oscillators, samplers, and basic effects.

pub struct SamplerVoice {
    pub pitch: f32,
    pub volume: f32,
    pub is_playing: bool,
}

impl Default for SamplerVoice {
    fn default() -> Self {
        Self {
            pitch: 1.0,
            volume: 1.0,
            is_playing: false,
        }
    }
}
