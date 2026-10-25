use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

const TICK: Duration = Duration::from_millis(20);
const SLACK_MS: u64 = 40;
const SAMPLE_BYTES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFormat {
    pub rate: u32,
    pub channels: u16,
}

pub struct Feed {
    stop: Arc<AtomicBool>,
}

impl Drop for Feed {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

pub fn format() -> Option<AudioFormat> {
    let device = cpal::default_host().default_output_device()?;
    let config = device.default_output_config().ok()?;
    if config.sample_format() != cpal::SampleFormat::F32 {
        tracing::warn!(
            "loopback audio skipped: {:?} output",
            config.sample_format()
        );
        return None;
    }
    Some(AudioFormat {
        rate: config.sample_rate(),
        channels: config.channels(),
    })
}

pub fn silence_frames(elapsed: Duration, rate: u32, written: u64) -> u64 {
    let expected =
        u64::try_from(elapsed.as_micros() * u128::from(rate) / 1_000_000).unwrap_or(u64::MAX);
    let slack = u64::from(rate) * SLACK_MS / 1000;
    if expected > written + slack {
        expected - written
    } else {
        0
    }
}

pub fn sample_bytes(samples: &[f32]) -> Vec<u8> {
    samples.iter().flat_map(|s| s.to_le_bytes()).collect()
}

pub fn start(format: AudioFormat, sink: impl Write + Send + 'static) -> Feed {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let spawned = std::thread::Builder::new()
        .name("aw-loopback".into())
        .spawn(move || {
            if let Err(error) = pump(format, sink, &flag) {
                tracing::warn!("loopback audio stopped: {error}");
            }
        });
    if let Err(error) = spawned {
        tracing::warn!("loopback thread failed: {error}");
    }
    Feed { stop }
}

fn pump(format: AudioFormat, mut sink: impl Write, stop: &AtomicBool) -> Result<(), String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("no output device")?;
    let config = cpal::StreamConfig {
        channels: format.channels,
        sample_rate: format.rate,
        buffer_size: cpal::BufferSize::Default,
    };
    let (tx, rx) = channel::<Vec<f32>>();
    let stream = device
        .build_input_stream::<f32, _, _>(
            config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let _ = tx.send(data.to_vec());
            },
            |error| tracing::warn!("loopback audio error: {error}"),
            None,
        )
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    let channels = usize::from(format.channels.max(1));
    let started = Instant::now();
    let mut written: u64 = 0;
    while !stop.load(Ordering::SeqCst) {
        let mut chunk = match rx.recv_timeout(TICK) {
            Ok(samples) => samples,
            Err(RecvTimeoutError::Timeout) => Vec::new(),
            Err(RecvTimeoutError::Disconnected) => break,
        };
        chunk.extend(rx.try_iter().flatten());
        let frames = u64::try_from(chunk.len() / channels).unwrap_or(0);
        let gap = silence_frames(started.elapsed(), format.rate, written + frames);
        let silence = usize::try_from(gap).unwrap_or(0) * channels * SAMPLE_BYTES;
        let result = sink
            .write_all(&vec![0; silence])
            .and_then(|()| sink.write_all(&sample_bytes(&chunk)));
        if let Err(error) = result {
            return if stop.load(Ordering::SeqCst) {
                Ok(())
            } else {
                Err(error.to_string())
            };
        }
        written += gap + frames;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_fills_only_gaps_longer_than_the_slack() {
        let second = Duration::from_secs(1);
        assert_eq!(silence_frames(second, 48_000, 48_000), 0);
        assert_eq!(silence_frames(second, 48_000, 47_000), 0);
        assert_eq!(silence_frames(second, 48_000, 40_000), 8_000);
        assert_eq!(silence_frames(second, 48_000, 50_000), 0);
        assert_eq!(
            silence_frames(Duration::from_millis(500), 44_100, 0),
            22_050
        );
    }

    #[test]
    fn samples_become_little_endian_bytes() {
        assert_eq!(
            sample_bytes(&[1.0, -0.5]),
            [1.0f32.to_le_bytes(), (-0.5f32).to_le_bytes()].concat()
        );
    }
}
