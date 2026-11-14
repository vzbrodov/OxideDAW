//! Oxide Audio - Real-time audio engine, driver management, and lock-free rings.

pub struct AudioEngineConfig {
    pub sample_rate: u32,
    pub buffer_size: u32,
}

impl Default for AudioEngineConfig {
    fn default() -> Self {
        Self {
            sample_rate: 44100,
            buffer_size: 256,
        }
    }
}
