//! Decoding runs on the control thread, before audio playback starts.
use std::{error::Error, fs::File, io::ErrorKind, path::Path};
use symphonia::core::{
    audio::SampleBuffer, codecs::DecoderOptions, errors::Error as DecodeError,
    formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
};

pub type SampleResult<T> = Result<T, Box<dyn Error>>;

pub struct AudioSample {
    pub samples: Vec<f32>,
    pub channels: usize,
    pub sample_rate: u32,
}

impl AudioSample {
    pub fn load(path: &Path) -> SampleResult<Self> {
        let source = MediaSourceStream::new(Box::new(File::open(path)?), Default::default());
        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|s| s.to_str()) {
            hint.with_extension(extension);
        }
        let mut format = symphonia::default::get_probe()
            .format(
                &hint,
                source,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )?
            .format;
        let track = format.default_track().ok_or("No default audio track")?;
        let id = track.id;
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())?;
        let mut result = Self {
            samples: Vec::new(),
            channels: 0,
            sample_rate: 0,
        };
        loop {
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(DecodeError::IoError(err)) if err.kind() == ErrorKind::UnexpectedEof => break,
                Err(err) => return Err(err.into()),
            };
            if packet.track_id() != id {
                continue;
            }
            let decoded = decoder.decode(&packet)?;
            let spec = *decoded.spec();
            if result.channels != 0
                && (result.channels != spec.channels.count() || result.sample_rate != spec.rate)
            {
                return Err("Audio format changed during decoding".into());
            }
            result.channels = spec.channels.count();
            result.sample_rate = spec.rate;
            let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
            buffer.copy_interleaved_ref(decoded);
            result.samples.extend_from_slice(buffer.samples());
        }
        if result.samples.is_empty()
            || result.sample_rate == 0
            || !(1..=2).contains(&result.channels)
        {
            return Err("Expected non-empty mono or stereo audio".into());
        }
        Ok(result)
    }
}
