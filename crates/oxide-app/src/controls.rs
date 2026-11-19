use oxide_audio::playback::PlaybackCommand;

pub fn parse(line: &str) -> Result<PlaybackCommand, &'static str> {
    let mut words = line.split_whitespace();
    let command = match (words.next(), words.next()) {
        (Some("play"), None) => PlaybackCommand::Play,
        (Some("pause"), None) => PlaybackCommand::Pause,
        (Some("stop"), None) => PlaybackCommand::Stop,
        (Some("bpm"), Some(value)) => {
            let bpm: f64 = value
                .parse()
                .map_err(|_| "Expected BPM between 1 and 999")?;
            if !bpm.is_finite() || !(1.0..=999.0).contains(&bpm) {
                return Err("Expected BPM between 1 and 999");
            }
            PlaybackCommand::SetBpm(bpm)
        }
        (Some("seek"), Some(value)) => {
            let step = value
                .parse()
                .map_err(|_| "Expected a nonnegative step index")?;
            PlaybackCommand::SeekStep(step)
        }
        _ => return Err("Expected play, pause, stop, bpm N, seek N, or quit"),
    };
    if words.next().is_some() {
        return Err("Unexpected trailing arguments");
    }
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_transport_controls_and_rejects_invalid_input() {
        assert_eq!(parse(" bpm 137.25 "), Ok(PlaybackCommand::SetBpm(137.25)));
        assert_eq!(parse("seek 32"), Ok(PlaybackCommand::SeekStep(32)));
        assert_eq!(parse("play"), Ok(PlaybackCommand::Play));
        for line in [
            "bpm NaN",
            "bpm inf",
            "bpm 0",
            "bpm 1000",
            "seek -1",
            "seek 1.5",
            "pause now",
            "bpm 120 extra",
            "unknown",
            "",
        ] {
            assert!(parse(line).is_err(), "{line}");
        }
    }
}
