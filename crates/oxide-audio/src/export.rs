//! Offline export through the same sequencer and mixer as live playback.
use crate::pattern::PatternRenderer;
use oxide_core::{sequencer::TRACKS, storage::write_file, Project};
use oxide_dsp::sample::AudioSample;
use std::{
    io::{self, BufWriter, Seek, Write},
    path::Path,
};

const BLOCK_FRAMES: usize = 1024;
const BYTES_PER_FRAME: u32 = 4;
const RIFF_OVERHEAD: u32 = 36;
const MAX_FRAMES: u64 = ((u32::MAX - RIFF_OVERHEAD) / BYTES_PER_FRAME) as u64;
const TOO_LONG: &str = "Rendered audio exceeds the RIFF WAV size limit";

/// Export one full pattern and its sample tails as stereo 16-bit PCM.
/// The destination is published only after rendering and is never overwritten.
pub fn export_pattern(
    project: &Project,
    samples: [AudioSample; TRACKS],
    destination: &Path,
) -> io::Result<()> {
    let rate = project.sample_rate;
    let byte_rate = rate
        .checked_mul(BYTES_PER_FRAME)
        .ok_or_else(|| io::Error::other("Sample rate exceeds the WAV byte rate limit"))?;
    let mut renderer = PatternRenderer::new(
        samples,
        project.pattern.clone(),
        rate,
        project.bpm,
        project.master_volume,
    )
    .map_err(io::Error::other)?;
    renderer.set_looping(false);
    let pattern_frames = renderer.one_shot_end_frame();
    if pattern_frames > MAX_FRAMES {
        return Err(io::Error::other(TOO_LONG));
    }
    renderer.play();
    write_file(destination, false, |file| {
        let mut wav = BufWriter::new(file);
        write_header(&mut wav, rate, byte_rate, 0)?;
        let mut block = [[0.0; 2]; BLOCK_FRAMES];
        let mut written = 0;
        while written < pattern_frames {
            let count = (pattern_frames - written).min(BLOCK_FRAMES as u64) as usize;
            let frames = &mut block[..count];
            renderer.render(frames);
            write_frames(&mut wav, frames)?;
            written += count as u64;
        }
        loop {
            let count = renderer.render_tail(&mut block);
            written += count as u64;
            if written > MAX_FRAMES {
                return Err(io::Error::other(TOO_LONG));
            }
            write_frames(&mut wav, &block[..count])?;
            if count < BLOCK_FRAMES {
                break;
            }
        }
        wav.rewind()?;
        write_header(&mut wav, rate, byte_rate, written as u32 * BYTES_PER_FRAME)?;
        wav.flush()
    })
}

fn write_header(mut wav: impl Write, rate: u32, byte_rate: u32, data_size: u32) -> io::Result<()> {
    wav.write_all(b"RIFF")?;
    wav.write_all(&(data_size + RIFF_OVERHEAD).to_le_bytes())?;
    wav.write_all(b"WAVEfmt ")?;
    wav.write_all(&16_u32.to_le_bytes())?; // Format chunk size.
    wav.write_all(&1_u16.to_le_bytes())?; // PCM.
    wav.write_all(&2_u16.to_le_bytes())?; // Stereo.
    wav.write_all(&rate.to_le_bytes())?;
    wav.write_all(&byte_rate.to_le_bytes())?;
    wav.write_all(&(BYTES_PER_FRAME as u16).to_le_bytes())?;
    wav.write_all(&16_u16.to_le_bytes())?; // Bits per sample.
    wav.write_all(b"data")?;
    wav.write_all(&data_size.to_le_bytes())
}

fn write_frames(mut wav: impl Write, frames: &[[f32; 2]]) -> io::Result<()> {
    for &sample in frames.iter().flatten() {
        let pcm = (sample * 32768.0).round() as i16;
        wav.write_all(&pcm.to_le_bytes())?;
    }
    Ok(())
}
