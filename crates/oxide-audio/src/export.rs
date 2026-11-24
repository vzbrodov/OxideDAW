//! Offline export through the same sequencer and mixer as live playback.
use crate::pattern::PatternRenderer;
use oxide_core::{
    sequencer::{PlaybackBar, TRACKS},
    storage::write_file,
    Project,
};
use oxide_dsp::sample::AudioSample;
use std::{
    io::{self, BufWriter, Seek, Write},
    path::Path,
};

const BLOCK_FRAMES: usize = 1024;
const TOO_LONG: &str = "Rendered audio exceeds the RIFF WAV size limit";

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum WavEncoding {
    #[default]
    Pcm16,
    Pcm24,
    Float32,
}

impl WavEncoding {
    fn bits_per_sample(self) -> u16 {
        match self {
            Self::Pcm16 => 16,
            Self::Pcm24 => 24,
            Self::Float32 => 32,
        }
    }

    fn bytes_per_frame(self) -> u32 {
        u32::from(self.bits_per_sample() / 8) * 2
    }

    fn riff_overhead(self) -> u32 {
        match self {
            Self::Pcm16 | Self::Pcm24 => 36,
            Self::Float32 => 50, // WAVEFORMATEX extension size and fact chunk.
        }
    }
}

/// Export one full pattern and its sample tails as stereo 16-bit PCM.
/// The destination is published only after rendering and is never overwritten.
pub fn export_pattern(
    project: &Project,
    samples: [AudioSample; TRACKS],
    destination: &Path,
) -> io::Result<()> {
    export(project, samples, &[], destination, WavEncoding::Pcm16)
}

/// Export the saved arrangement, or one main pattern when the arrangement is empty.
pub fn export_project(
    project: &Project,
    samples: [AudioSample; TRACKS],
    destination: &Path,
) -> io::Result<()> {
    export_project_as(project, samples, destination, WavEncoding::Pcm16)
}

/// Export the saved arrangement or main pattern using the selected WAV encoding.
/// The destination is published only after rendering and is never overwritten.
pub fn export_project_as(
    project: &Project,
    samples: [AudioSample; TRACKS],
    destination: &Path,
    encoding: WavEncoding,
) -> io::Result<()> {
    let bars = project
        .playback_bars()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
    export(project, samples, &bars, destination, encoding)
}

fn export(
    project: &Project,
    samples: [AudioSample; TRACKS],
    arrangement: &[PlaybackBar],
    destination: &Path,
    encoding: WavEncoding,
) -> io::Result<()> {
    let rate = project.sample_rate;
    let max_frames = u64::from((u32::MAX - encoding.riff_overhead()) / encoding.bytes_per_frame());
    let mut renderer = PatternRenderer::new(
        samples,
        project.pattern.clone(),
        rate,
        project.bpm,
        project.master_volume,
    )
    .map_err(io::Error::other)?;
    renderer.set_looping(false);
    let sequence_frames = renderer.sequence_end_frame(arrangement);
    if sequence_frames > max_frames {
        return Err(io::Error::other(TOO_LONG));
    }
    renderer.play();
    write_file(destination, false, |file| {
        let mut wav = BufWriter::new(file);
        write_header(&mut wav, rate, encoding, 0)?;
        let mut block = [[0.0; 2]; BLOCK_FRAMES];
        let mut written = 0;
        while written < sequence_frames {
            let count = (sequence_frames - written).min(BLOCK_FRAMES as u64) as usize;
            let frames = &mut block[..count];
            renderer.render_sequence(frames, arrangement);
            write_frames(&mut wav, frames, encoding)?;
            written += count as u64;
        }
        loop {
            let count = renderer.render_tail(&mut block);
            written += count as u64;
            if written > max_frames {
                return Err(io::Error::other(TOO_LONG));
            }
            write_frames(&mut wav, &block[..count], encoding)?;
            if count < BLOCK_FRAMES {
                break;
            }
        }
        wav.rewind()?;
        write_header(&mut wav, rate, encoding, written as u32)?;
        wav.flush()
    })
}

fn write_header(
    mut wav: impl Write,
    rate: u32,
    encoding: WavEncoding,
    frames: u32,
) -> io::Result<()> {
    let bytes_per_frame = encoding.bytes_per_frame();
    let byte_rate = rate
        .checked_mul(bytes_per_frame)
        .ok_or_else(|| io::Error::other("Sample rate exceeds the WAV byte rate limit"))?;
    let data_size = frames * bytes_per_frame;
    let (format_size, format_tag) = match encoding {
        WavEncoding::Pcm16 | WavEncoding::Pcm24 => (16_u32, 1_u16),
        WavEncoding::Float32 => (18_u32, 3_u16),
    };
    wav.write_all(b"RIFF")?;
    wav.write_all(&(data_size + encoding.riff_overhead()).to_le_bytes())?;
    wav.write_all(b"WAVEfmt ")?;
    wav.write_all(&format_size.to_le_bytes())?;
    wav.write_all(&format_tag.to_le_bytes())?;
    wav.write_all(&2_u16.to_le_bytes())?; // Stereo.
    wav.write_all(&rate.to_le_bytes())?;
    wav.write_all(&byte_rate.to_le_bytes())?;
    wav.write_all(&(bytes_per_frame as u16).to_le_bytes())?;
    wav.write_all(&encoding.bits_per_sample().to_le_bytes())?;
    if matches!(encoding, WavEncoding::Float32) {
        wav.write_all(&0_u16.to_le_bytes())?; // No additional format data.
        wav.write_all(b"fact")?;
        wav.write_all(&4_u32.to_le_bytes())?;
        wav.write_all(&frames.to_le_bytes())?;
    }
    wav.write_all(b"data")?;
    wav.write_all(&data_size.to_le_bytes())
}

fn write_frames(mut wav: impl Write, frames: &[[f32; 2]], encoding: WavEncoding) -> io::Result<()> {
    for &sample in frames.iter().flatten() {
        match encoding {
            WavEncoding::Pcm16 => {
                let pcm = (sample * 32768.0).round() as i16;
                wav.write_all(&pcm.to_le_bytes())?;
            }
            WavEncoding::Pcm24 => {
                let pcm = ((sample * 8388608.0).round() as i32).clamp(-8388608, 8388607);
                wav.write_all(&pcm.to_le_bytes()[..3])?;
            }
            WavEncoding::Float32 => wav.write_all(&sample.to_le_bytes())?,
        }
    }
    Ok(())
}
