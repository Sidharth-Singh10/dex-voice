//! Device I/O and the bounded PCM primitives the engine needs.

mod trimmer;

pub use trimmer::Trimmer;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use cpal::FromSample;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Linear resampler with persistent fractional position and one-sample history.
#[derive(Debug)]
pub struct Resampler {
    step: f64,
    position: f64,
    history: Vec<f32>,
}

impl Resampler {
    pub fn new(source_rate: u32, target_rate: u32) -> Result<Self> {
        if source_rate == 0 || target_rate == 0 {
            return Err(anyhow!("sample rates must be positive"));
        }
        Ok(Self {
            step: source_rate as f64 / target_rate as f64,
            position: 0.0,
            history: Vec::with_capacity(1),
        })
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }
        let mut samples = Vec::with_capacity(self.history.len() + input.len());
        samples.extend_from_slice(&self.history);
        samples.extend_from_slice(input);
        let end = samples.len().saturating_sub(1);
        let mut output = Vec::new();
        let mut position = self.position;
        while position + 1e-9 < end as f64 {
            let index = position.floor() as usize;
            let fraction = (position - index as f64) as f32;
            let left = samples[index];
            let right = samples[index + 1];
            output.push(left * (1.0 - fraction) + right * fraction);
            position += self.step;
        }
        // Keep the last input sample as the left side of the next chunk's
        // interpolation, and keep the next output's phase relative to it.
        // Using floor(position) here resets phase when a step skips past a
        // chunk boundary, producing extra samples for split input.
        self.history = vec![samples[end]];
        self.position = position - end as f64;
        output
    }
}

/// Bounded mono f32 queue. Overflow is counted, never hidden.
#[derive(Debug, Default)]
pub struct PcmQueue {
    samples: Mutex<VecDeque<f32>>,
    capacity: usize,
    dropped: AtomicU64,
    pops: AtomicU64,
    /// Updated only when a pop actually returned samples. A consumer that is
    /// running but starving must not look like useful output work.
    last_pop: Mutex<Option<Instant>>,
}

impl PcmQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
            dropped: AtomicU64::new(0),
            pops: AtomicU64::new(0),
            last_pop: Mutex::new(None),
        }
    }

    pub fn push(&self, input: &[f32]) -> usize {
        let mut samples = self.samples.lock().unwrap_or_else(|e| e.into_inner());
        let free = self.capacity.saturating_sub(samples.len());
        let take = free.min(input.len());
        samples.extend(input[..take].iter().copied());
        let dropped = input.len() - take;
        if dropped > 0 {
            self.dropped.fetch_add(dropped as u64, Ordering::Relaxed);
        }
        dropped
    }

    /// Fills `output` with queued samples, zero-padding an underrun.
    pub fn pop_into(&self, output: &mut [f32]) -> usize {
        self.pop_into_at(output, Instant::now())
    }

    /// Test/clock-injected pop. The liveness timestamp uses the caller's clock
    /// so a hermetic test can advance time without sleeping.
    pub fn pop_into_at(&self, output: &mut [f32], now: Instant) -> usize {
        let mut samples = self.samples.lock().unwrap_or_else(|e| e.into_inner());
        let count = output.len().min(samples.len());
        for (slot, sample) in output.iter_mut().zip(samples.drain(..count)) {
            *slot = sample;
        }
        output[count..].fill(0.0);
        if count > 0 {
            self.pops.fetch_add(1, Ordering::Relaxed);
            *self.last_pop.lock().unwrap_or_else(|e| e.into_inner()) = Some(now);
        }
        count
    }

    pub fn len(&self) -> usize {
        self.samples.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    pub fn pops(&self) -> u64 {
        self.pops.load(Ordering::Relaxed)
    }

    /// Timestamp of the last pop that returned audio, if any.
    pub fn last_pop(&self) -> Option<Instant> {
        *self.last_pop.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Discards queued audio. Returns the number of samples removed. Used when a
    /// cancelled phrase must not leave late PCM behind.
    pub fn clear(&self) -> usize {
        let mut samples = self.samples.lock().unwrap_or_else(|e| e.into_inner());
        let count = samples.len();
        samples.clear();
        count
    }
}

pub fn input_devices() -> Result<Vec<String>> {
    Ok(cpal::default_host()
        .input_devices()
        .context("enumerate input devices")?
        .map(|device| device.to_string())
        .collect())
}

/// ID is stable across runs when the host supports it; labels may repeat.
#[derive(Clone, Debug)]
pub struct DeviceChoice {
    pub id: String,
    pub label: String,
}

pub fn device_choices(input: bool) -> Result<Vec<DeviceChoice>> {
    let host = cpal::default_host();
    let devices = if input {
        host.input_devices().context("enumerate input devices")?
    } else {
        host.output_devices().context("enumerate output devices")?
    };
    Ok(devices
        .filter_map(|device| {
            Some(DeviceChoice {
                id: device.id().ok()?.to_string(),
                label: device.to_string(),
            })
        })
        .collect())
}

pub fn output_devices() -> Result<Vec<String>> {
    Ok(cpal::default_host()
        .output_devices()
        .context("enumerate output devices")?
        .map(|device| device.to_string())
        .collect())
}

/// The default output device's sample rate, so the engine can produce audio at
/// exactly the rate the playback stream will consume.
pub fn output_sample_rate(device_name: &str) -> Result<u32> {
    let host = cpal::default_host();
    let device = find_device(&host, device_name, false)?;
    Ok(device
        .default_output_config()
        .context("default output config")?
        .sample_rate())
}

fn find_device(host: &cpal::Host, name: &str, input: bool) -> Result<cpal::Device> {
    let wanted = name.trim().to_lowercase();
    if wanted.is_empty() {
        return if input {
            host.default_input_device()
                .context("no default input device")
        } else {
            host.default_output_device()
                .context("no default output device")
        };
    }
    let devices = if input {
        host.input_devices().context("enumerate input devices")?
    } else {
        host.output_devices().context("enumerate output devices")?
    };
    let devices: Vec<_> = devices.collect();
    devices
        .iter()
        .find(|device| device.id().is_ok_and(|id| id.to_string() == name))
        .or_else(|| {
            devices
                .iter()
                .find(|device| device.to_string().eq_ignore_ascii_case(name))
        })
        .or_else(|| {
            devices
                .iter()
                .find(|device| device.to_string().to_lowercase().contains(&wanted))
        })
        .cloned()
        .with_context(|| format!("no device matching {name:?}"))
}

fn mono_f32<T>(data: &[T], channels: usize) -> Vec<f32>
where
    T: Copy,
    f32: cpal::FromSample<T>,
{
    data.chunks(channels)
        .map(|frame| {
            frame
                .iter()
                .map(|&sample| f32::from_sample_(sample))
                .sum::<f32>()
                / channels as f32
        })
        .collect()
}

/// Builds a capture stream for any sample format `cpal` can convert to f32.
fn input_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    mut on_block: impl FnMut(Vec<f32>) + Send + 'static,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample + Copy + Send + 'static,
    f32: cpal::FromSample<T>,
{
    device
        .build_input_stream(
            config,
            move |data: &[T], _| on_block(mono_f32(data, channels)),
            |error| eprintln!("capture stream error: {error}"),
            None,
        )
        .context("build capture stream")
}

/// Builds a playback stream for any sample format `cpal` can convert from f32.
fn output_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    queue: Arc<PcmQueue>,
    health: std::sync::Arc<PlaybackHealth>,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample + Copy + Send + 'static,
    T: cpal::FromSample<f32>,
{
    let mut mono = Vec::new();
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                mono.resize(data.len() / channels, 0.0);
                queue.pop_into(&mut mono);
                for (frame, &sample) in data.chunks_mut(channels).zip(mono.iter()) {
                    frame.fill(T::from_sample_(sample));
                }
            },
            {
                let health = std::sync::Arc::clone(&health);
                move |error| {
                    eprintln!("playback stream error: {error}");
                    health.set_error(error.to_string());
                }
            },
            None,
        )
        .context("build playback stream")
}

/// Stream config with a small fixed buffer. PulseAudio's default buffer can
/// hold seconds of audio; that adds latency and keeps our own speech audible
/// long after the queue drains, which feeds the half-duplex echo loop.
fn requested_config(config: cpal::SupportedStreamConfig) -> cpal::StreamConfig {
    let supported = *config.buffer_size();
    let rate = config.sample_rate();
    let mut stream: cpal::StreamConfig = config.into();
    if let cpal::SupportedBufferSize::Range { min, max } = supported {
        stream.buffer_size = cpal::BufferSize::Fixed((rate / 50).clamp(min, max));
    }
    stream
}

/// Keeps a capture stream alive. Drop stops it.
pub struct Capture {
    _stream: cpal::Stream,
    pub sample_rate: u32,
    pub channels: u16,
}

pub fn start_capture(
    device_name: &str,
    on_block: impl FnMut(Vec<f32>) + Send + 'static,
) -> Result<Capture> {
    let host = cpal::default_host();
    let device = find_device(&host, device_name, true)?;
    let config = device
        .default_input_config()
        .context("default input config")?;
    let sample_rate = config.sample_rate();
    let channels = config.channels() as usize;
    eprintln!(
        "audio: capture {:?} at {sample_rate} Hz, {channels} ch",
        device.to_string()
    );
    let format = config.sample_format();
    let stream_config = requested_config(config);
    let stream = match format {
        cpal::SampleFormat::F32 => input_stream::<f32>(&device, stream_config, channels, on_block)?,
        cpal::SampleFormat::F64 => input_stream::<f64>(&device, stream_config, channels, on_block)?,
        cpal::SampleFormat::I8 => input_stream::<i8>(&device, stream_config, channels, on_block)?,
        cpal::SampleFormat::I16 => input_stream::<i16>(&device, stream_config, channels, on_block)?,
        cpal::SampleFormat::I24 => {
            input_stream::<cpal::I24>(&device, stream_config, channels, on_block)?
        }
        cpal::SampleFormat::I32 => input_stream::<i32>(&device, stream_config, channels, on_block)?,
        cpal::SampleFormat::I64 => input_stream::<i64>(&device, stream_config, channels, on_block)?,
        cpal::SampleFormat::U8 => input_stream::<u8>(&device, stream_config, channels, on_block)?,
        cpal::SampleFormat::U16 => input_stream::<u16>(&device, stream_config, channels, on_block)?,
        cpal::SampleFormat::U24 => {
            input_stream::<cpal::U24>(&device, stream_config, channels, on_block)?
        }
        cpal::SampleFormat::U32 => input_stream::<u32>(&device, stream_config, channels, on_block)?,
        cpal::SampleFormat::U64 => input_stream::<u64>(&device, stream_config, channels, on_block)?,
        other => return Err(anyhow!("unsupported input sample format {other:?}")),
    };
    stream.play().context("start capture")?;
    Ok(Capture {
        _stream: stream,
        sample_rate,
        channels: config.channels(),
    })
}

/// Shared health state for a playback stream. The stream's error callback
/// records a failure here so the app can observe it without joining.
#[derive(Default)]
pub struct PlaybackHealth {
    pub error: std::sync::Mutex<Option<String>>,
}

impl PlaybackHealth {
    pub fn take_error(&self) -> Option<String> {
        self.error.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
    pub fn has_error(&self) -> bool {
        self.error.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }
    pub fn set_error(&self, message: String) {
        *self.error.lock().unwrap_or_else(|e| e.into_inner()) = Some(message);
    }
}

/// Keeps a playback stream alive. Drop stops it.
pub struct Playback {
    _stream: cpal::Stream,
    pub sample_rate: u32,
    pub channels: u16,
    pub health: std::sync::Arc<PlaybackHealth>,
}

pub fn start_playback(device_name: &str, queue: Arc<PcmQueue>) -> Result<Playback> {
    let host = cpal::default_host();
    let device = find_device(&host, device_name, false)?;
    let config = device
        .default_output_config()
        .context("default output config")?;
    let sample_rate = config.sample_rate();
    let channels = config.channels() as usize;
    eprintln!(
        "audio: playback {:?} at {sample_rate} Hz, {channels} ch",
        device.to_string()
    );
    let format = config.sample_format();
    let stream_config = requested_config(config);
    let health = std::sync::Arc::new(PlaybackHealth::default());
    let stream = match format {
        cpal::SampleFormat::F32 => output_stream::<f32>(&device, stream_config, channels, queue, std::sync::Arc::clone(&health))?,
        cpal::SampleFormat::F64 => output_stream::<f64>(&device, stream_config, channels, queue, std::sync::Arc::clone(&health))?,
        cpal::SampleFormat::I8 => output_stream::<i8>(&device, stream_config, channels, queue, std::sync::Arc::clone(&health))?,
        cpal::SampleFormat::I16 => output_stream::<i16>(&device, stream_config, channels, queue, std::sync::Arc::clone(&health))?,
        cpal::SampleFormat::I24 => {
            output_stream::<cpal::I24>(&device, stream_config, channels, queue, std::sync::Arc::clone(&health))?
        }
        cpal::SampleFormat::I32 => output_stream::<i32>(&device, stream_config, channels, queue, std::sync::Arc::clone(&health))?,
        cpal::SampleFormat::I64 => output_stream::<i64>(&device, stream_config, channels, queue, std::sync::Arc::clone(&health))?,
        cpal::SampleFormat::U8 => output_stream::<u8>(&device, stream_config, channels, queue, std::sync::Arc::clone(&health))?,
        cpal::SampleFormat::U16 => output_stream::<u16>(&device, stream_config, channels, queue, std::sync::Arc::clone(&health))?,
        cpal::SampleFormat::U24 => {
            output_stream::<cpal::U24>(&device, stream_config, channels, queue, std::sync::Arc::clone(&health))?
        }
        cpal::SampleFormat::U32 => output_stream::<u32>(&device, stream_config, channels, queue, std::sync::Arc::clone(&health))?,
        cpal::SampleFormat::U64 => output_stream::<u64>(&device, stream_config, channels, queue, std::sync::Arc::clone(&health))?,
        other => return Err(anyhow!("unsupported output sample format {other:?}")),
    };
    stream.play().context("start playback")?;
    Ok(Playback {
        _stream: stream,
        sample_rate,
        channels: config.channels(),
        health,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn resampler_is_continuous_across_chunks() {
        let signal: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.05).sin()).collect();
        let mut once = Resampler::new(48000, 16000).unwrap();
        let expected = once.process(&signal);
        let mut split = Resampler::new(48000, 16000).unwrap();
        let mut actual = Vec::new();
        for chunk in signal.chunks(317) {
            actual.extend(split.process(chunk));
        }
        assert_eq!(actual.len(), expected.len());
        for (a, b) in actual.iter().zip(expected.iter()) {
            assert!((a - b).abs() < 1e-6, "{a} vs {b}");
        }
    }

    #[test]
    fn resampler_keeps_phase_for_small_chunks_and_upsampling() {
        let signal: Vec<f32> = (0..101).map(|i| (i as f32 * 0.1).sin()).collect();
        for (source, target) in [(48_000, 16_000), (16_000, 48_000), (44_100, 48_000)] {
            let expected = Resampler::new(source, target).unwrap().process(&signal);
            for width in [1, 2, 7, 29] {
                let mut resampler = Resampler::new(source, target).unwrap();
                let actual: Vec<_> = signal
                    .chunks(width)
                    .flat_map(|chunk| resampler.process(chunk))
                    .collect();
                assert_eq!(
                    actual.len(),
                    expected.len(),
                    "{source} -> {target}, chunk {width}"
                );
                for (a, b) in actual.iter().zip(expected.iter()) {
                    assert!((a - b).abs() < 1e-5, "{source} -> {target}: {a} vs {b}");
                }
            }
        }
    }

    #[test]
    fn queue_reports_overflow_and_pads_underrun() {
        let queue = PcmQueue::new(2);
        assert_eq!(queue.push(&[1.0, 2.0, 3.0]), 1);
        assert_eq!(queue.dropped(), 1);
        let mut output = [0.0; 4];
        assert_eq!(queue.pop_into(&mut output), 2);
        assert_eq!(output, [1.0, 2.0, 0.0, 0.0]);
    }

    #[test]
    fn last_pop_only_advances_when_samples_were_returned() {
        let queue = PcmQueue::new(4);
        let now = Instant::now();
        let mut output = [0.0; 4];
        assert_eq!(queue.pop_into_at(&mut output, now), 0);
        assert_eq!(queue.last_pop(), None);
        queue.push(&[1.0, 2.0]);
        assert_eq!(
            queue.pop_into_at(&mut output, now + Duration::from_millis(20)),
            2
        );
        assert_eq!(queue.last_pop(), Some(now + Duration::from_millis(20)));
        assert_eq!(
            queue.pop_into_at(&mut output, now + Duration::from_millis(40)),
            0
        );
        assert_eq!(queue.last_pop(), Some(now + Duration::from_millis(20)));
        assert_eq!(queue.pops(), 1);
    }

    #[test]
    fn clear_counts_and_discards_queued_audio() {
        let queue = PcmQueue::new(8);
        queue.push(&[1.0; 5]);
        assert_eq!(queue.clear(), 5);
        assert!(queue.is_empty());
        assert_eq!(queue.clear(), 0);
    }
}
