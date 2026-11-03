use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const RATE: u32 = 22_050;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    Click,
    Step,
    Celebrate,
}

fn voice(time: f32, start: f32, duration: f32, frequency: f32, gain: f32, triangle: bool) -> f32 {
    let elapsed = time - start;
    if !(0.0..duration).contains(&elapsed) {
        return 0.0;
    }
    let phase = elapsed * frequency;
    let wave = if triangle {
        2.0 * (2.0 * (phase - (phase + 0.5).floor())).abs() - 1.0
    } else {
        (phase * std::f32::consts::TAU).sin()
    };
    let decay = (0.001_f32 / gain).powf(elapsed / duration);
    let attack = (elapsed / 0.003).min(1.0);
    wave * gain * decay * attack
}

fn samples(sound: Sound) -> Vec<i16> {
    let duration = match sound {
        Sound::Click => 0.035,
        Sound::Step => 0.26,
        Sound::Celebrate => 0.80,
    };
    let count = (RATE as f32 * duration) as usize;
    (0..count)
        .map(|index| {
            let time = index as f32 / RATE as f32;
            let value = match sound {
                Sound::Click => {
                    let duration = 0.035;
                    let ratio: f32 = 200.0 / 750.0;
                    let phase = 750.0 * duration * (ratio.powf(time / duration) - 1.0) / ratio.ln();
                    let envelope = 0.08 * (0.001_f32 / 0.08).powf(time / duration) * (time / 0.003).min(1.0);
                    (phase * std::f32::consts::TAU).sin() * envelope
                }
                Sound::Step => [523.25, 659.25, 783.99]
                    .into_iter()
                    .enumerate()
                    .map(|(i, frequency)| voice(time, i as f32 * 0.03, 0.2, frequency, 0.05, true))
                    .sum(),
                Sound::Celebrate => [440.0, 554.37, 659.25, 880.0]
                    .into_iter()
                    .enumerate()
                    .map(|(i, frequency)| voice(time, i as f32 * 0.05, 0.6, frequency, 0.06, false))
                    .sum(),
            };
            (value.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16
        })
        .collect()
}

fn wav(sound: Sound) -> Vec<u8> {
    let samples = samples(sound);
    let size = u32::try_from(samples.len() * 2).expect("the effects are shorter than a second");
    let mut bytes = Vec::with_capacity(size as usize + 44);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(size + 36).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&(RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&size.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

pub fn play(sound: Sound) {
    static LAST_CLICK: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    if sound == Sound::Click {
        let Ok(mut last) = LAST_CLICK.get_or_init(|| Mutex::new(None)).try_lock() else { return };
        let now = Instant::now();
        if last.is_some_and(|time| now.duration_since(time) < Duration::from_millis(35)) {
            return;
        }
        *last = Some(now);
    }
    let _ = std::thread::Builder::new().name("glide-welcome-sound".into()).spawn(move || {
        let bytes = wav(sound);
        for player in ["pw-play", "paplay"] {
            let Ok(mut child) =
                Command::new(player).arg("-").stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
            else {
                continue;
            };
            let written = child.stdin.take().is_some_and(|mut input| input.write_all(&bytes).is_ok());
            if child.wait().is_ok_and(|status| status.success()) && written {
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_have_valid_wave_headers_and_distinct_durations() {
        let click = wav(Sound::Click);
        let step = wav(Sound::Step);
        let celebrate = wav(Sound::Celebrate);
        for bytes in [&click, &step, &celebrate] {
            assert_eq!(&bytes[..4], b"RIFF");
            assert_eq!(&bytes[8..12], b"WAVE");
            assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize, bytes.len() - 44);
            assert!(bytes[44..].iter().any(|sample| *sample != 0));
        }
        assert!(click.len() < step.len() && step.len() < celebrate.len());
    }
}
