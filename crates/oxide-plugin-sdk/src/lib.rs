//! Oxide Plugin SDK - Trait definitions for building custom native and WASM plugins.

pub trait OxidePlugin: Send + 'static {
    fn name(&self) -> &'static str;
    fn reset(&mut self, sample_rate: f32);
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], frames: usize);
}
