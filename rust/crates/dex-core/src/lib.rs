//! Realtime pipeline: Parakeet Unified (transcribe.cpp/Vulkan) → Phonon (CPU)
//! → bounded PCM queue. Device playback is the caller's job.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use dex_audio::{PcmQueue, Resampler, Trimmer};
use ptts::checkpoint::{Checkpoint, ResolveOptions};
use ptts::preprocess::Lang;
use ptts::synth::{DeviceKind, Quant, SpeechOptions};
use serde::Serialize;
use transcribe_cpp::{
    Backend, DeviceType, Model, ModelOptions, ParakeetBufferedStreamOptions, RunOptions,
    StreamExtension, StreamOptions, TimestampKind, devices, init_backends_default,
};

#[derive(Clone, Debug)]
pub struct EngineConfig {
    pub asr_model: PathBuf,
    pub tts_model: PathBuf,
    pub voice: String,
    pub device: String,
    pub left_ms: i32,
    pub chunk_ms: i32,
    pub right_ms: i32,
    pub group_words: usize,
    pub group_ms: u64,
    pub trim_tts: bool,
    pub trim_threshold: f32,
    pub output_rate: u32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            asr_model: PathBuf::new(),
            tts_model: PathBuf::new(),
            voice: "default".into(),
            device: "nvidia".into(),
            left_ms: 5600,
            chunk_ms: 160,
            right_ms: 80,
            group_words: 3,
            group_ms: 240,
            trim_tts: true,
            trim_threshold: 0.003,
            output_rate: 48_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Loading,
    Running,
    Stopping,
    #[default]
    Idle,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Status {
    pub state: State,
    pub error: Option<String>,
    pub transcript: String,
    pub words: usize,
    pub queued_seconds: f64,
    pub dropped_seconds: f64,
    pub input_drops: u64,
    pub group_delays: Vec<f64>,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    /// True while the microphone is ignored because our own voice is playing.
    pub input_paused: bool,
}

struct Group {
    words: Vec<String>,
    at: Instant,
}

/// Input gating state used to avoid recognizing our own synthesized speech.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Gate {
    /// Feeding the recognizer normally.
    Listening,
    /// Our own audio is queued or still draining; discard microphone input.
    Speaking,
    /// Speech finished; wait for quiet so residual echo is not recognized.
    Settling,
}

/// Linear RMS below which the microphone is treated as quiet.
const ECHO_QUIET: f32 = 0.01;

pub struct Engine {
    status: Arc<Mutex<Status>>,
    input: Mutex<Option<SyncSender<Vec<f32>>>>,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl Engine {
    pub fn start(config: EngineConfig, output: Arc<PcmQueue>) -> Result<Self> {
        if config.output_rate == 0 {
            bail!("output rate must be positive");
        }
        let status = Arc::new(Mutex::new(Status {
            state: State::Loading,
            ..Status::default()
        }));
        let (input_tx, input_rx) = sync_channel::<Vec<f32>>(200);
        let (group_tx, group_rx) = sync_channel::<Option<Group>>(64);
        let ready = Arc::new(AtomicUsize::new(0));

        let asr_thread = {
            let config = config.clone();
            let status = Arc::clone(&status);
            let ready = Arc::clone(&ready);
            let output = Arc::clone(&output);
            thread::Builder::new()
                .name("dex-asr".into())
                .spawn(move || asr_loop(config, input_rx, group_tx, status, ready, output))
                .context("spawn ASR thread")?
        };
        let tts_thread = {
            let config = config.clone();
            let status = Arc::clone(&status);
            let ready = Arc::clone(&ready);
            thread::Builder::new()
                .name("dex-tts".into())
                .spawn(move || tts_loop(config, group_rx, output, status, ready))
                .context("spawn TTS thread")?
        };
        Ok(Self {
            status,
            input: Mutex::new(Some(input_tx)),
            threads: Mutex::new(vec![asr_thread, tts_thread]),
        })
    }

    /// Feed 16 kHz mono PCM. Returns false when the input queue is full.
    pub fn push_input(&self, pcm: &[f32]) -> bool {
        let guard = self.input.lock().unwrap_or_else(|e| e.into_inner());
        let Some(sender) = guard.as_ref() else {
            return false;
        };
        match sender.try_send(pcm.to_vec()) {
            Ok(()) => true,
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                let mut status = self.lock();
                status.input_drops += 1;
                false
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => false,
        }
    }

    pub fn status(&self) -> Status {
        self.lock().clone()
    }

    /// Clears the displayed transcript without disturbing recognition.
    pub fn clear_transcript(&self) {
        let mut status = self.lock();
        status.transcript.clear();
        status.words = 0;
    }

    pub fn stop(&self) {
        drop(self.input.lock().unwrap_or_else(|e| e.into_inner()).take());
        let mut status = self.lock();
        if status.state != State::Failed {
            status.state = State::Stopping;
        }
        drop(status);
        let threads = std::mem::take(&mut *self.threads.lock().unwrap_or_else(|e| e.into_inner()));
        for thread in threads {
            let _ = thread.join();
        }
        let mut status = self.lock();
        if status.state != State::Failed {
            status.state = State::Idle;
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Status> {
        self.status.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}

fn ready(status: &Arc<Mutex<Status>>, ready: &AtomicUsize) {
    let count = ready.fetch_add(1, Ordering::SeqCst);
    eprintln!("engine: worker ready ({}/{})", count + 1, 2);
    if count == 1 {
        let mut status = status.lock().unwrap_or_else(|e| e.into_inner());
        if status.state == State::Loading {
            status.state = State::Running;
        }
    }
}

fn fail(status: &Arc<Mutex<Status>>, error: impl Into<String>) {
    let mut status = status.lock().unwrap_or_else(|e| e.into_inner());
    status.state = State::Failed;
    status.error = Some(error.into());
}

fn asr_loop(
    config: EngineConfig,
    input: Receiver<Vec<f32>>,
    groups: SyncSender<Option<Group>>,
    status: Arc<Mutex<Status>>,
    ready_count: Arc<AtomicUsize>,
    output: Arc<PcmQueue>,
) {
    if let Err(error) = asr_inner(&config, input, &groups, &status, &ready_count, &output) {
        fail(&status, format!("Recognition: {error:#}"));
    }
    let _ = groups.send(None);
}

/// Finalizes the current stream, queues anything it has recognized, and
/// returns false when the speech worker has gone away.
fn flush_stream(
    stream: &mut transcribe_cpp::Stream<'_>,
    seen: &mut usize,
    pending: &mut Vec<String>,
    first_at: &mut Option<Instant>,
    groups: &SyncSender<Option<Group>>,
    status: &Arc<Mutex<Status>>,
) -> bool {
    let _ = stream.finalize();
    // After finalize the raw hypothesis is authoritative, and the final word
    // may never have reached the committed prefix before the stream ended.
    let text = stream.text().full;
    let words = rest_words(&text, seen);
    pending.extend(words.iter().cloned());
    update_status(status, &words);
    if pending.is_empty() {
        return true;
    }
    let group = Group {
        words: std::mem::take(pending),
        at: first_at.take().unwrap_or_else(Instant::now),
    };
    groups.send(Some(group)).is_ok()
}

fn asr_inner(
    config: &EngineConfig,
    input: Receiver<Vec<f32>>,
    groups: &SyncSender<Option<Group>>,
    status: &Arc<Mutex<Status>>,
    ready_count: &AtomicUsize,
    output: &PcmQueue,
) -> Result<()> {
    init_backends_default().context("initialise transcribe.cpp backends")?;
    let available = devices();
    let wanted = config.device.to_lowercase();
    let device = available
        .iter()
        .filter(|device| device.device_type != DeviceType::Cpu)
        .find(|device| {
            wanted.is_empty()
                || device.description.to_lowercase().contains(&wanted)
                || device.name.to_lowercase().contains(&wanted)
        })
        .cloned()
        .ok_or_else(|| {
            anyhow!(
                "no GPU matching {:?}; saw {}",
                config.device,
                available
                    .iter()
                    .map(|d| d.description.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
    let model = Model::load_with(
        &config.asr_model,
        &ModelOptions {
            backend: Backend::Vulkan,
            device: Some(device),
            no_mmap: true,
        },
    )
    .with_context(|| format!("load {}", config.asr_model.display()))?;
    let mut session = model.session().context("create session")?;
    let run = RunOptions {
        timestamps: TimestampKind::Token,
        ..RunOptions::default()
    };
    let stream_options = StreamOptions {
        commit_policy: transcribe_cpp::CommitPolicy::Auto,
        stable_prefix_agreement_n: 0,
        family: Some(StreamExtension::ParakeetBuffered(
            ParakeetBufferedStreamOptions {
                left_ms: Some(config.left_ms),
                chunk_ms: Some(config.chunk_ms),
                right_ms: Some(config.right_ms),
            },
        )),
    };
    {
        let mut warm = session
            .stream(&run, &stream_options)
            .context("warmup stream")?;
        warm.feed(&vec![0.0; 8000]).context("warmup feed")?;
        warm.finalize().context("warmup finalize")?;
    }
    ready(status, ready_count);

    let mut stream = session
        .stream(&run, &stream_options)
        .context("start stream")?;
    let mut seen = 0usize;
    let mut pending: Vec<String> = Vec::new();
    let mut first_at: Option<Instant> = None;
    let mut silence_since: Option<Instant> = None;
    // Half-duplex: without echo cancellation, our own synthesized voice can
    // reach the microphone (through speakers or a looped virtual device) and be
    // recognized again, forever. Stop listening while output is still pending,
    // then wait for a genuinely quiet stretch before trusting the mic again.
    let mut gate = Gate::Listening;
    let mut quiet_since: Option<Instant> = None;
    let mut last_playing: Option<Instant> = None;
    // A talker who never pauses still needs to be heard eventually.
    let mut pending_since: Option<Instant> = None;
    loop {
        let block = match input.recv_timeout(Duration::from_millis(50)) {
            Ok(block) => block,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // The final word stays in the stream's lookahead until it is
                // finalized; the silence path below flushes it.
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let playing = !output.is_empty();
        if playing {
            last_playing = Some(Instant::now());
        }
        let speaking =
            playing || last_playing.is_some_and(|at| at.elapsed() < Duration::from_millis(250));
        if gate == Gate::Listening && speaking {
            gate = Gate::Speaking;
            // Flush what the stream has already read, then drop it so the echo
            // cannot leak into the next utterance.
            if !flush_stream(
                &mut stream,
                &mut seen,
                &mut pending,
                &mut first_at,
                groups,
                status,
            ) {
                break;
            }
            drop(stream);
            stream = session
                .stream(&run, &stream_options)
                .context("restart stream when speech starts")?;
            seen = 0;
            silence_since = None;
            quiet_since = None;
            status
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .input_paused = true;
            eprintln!("engine: listening paused while speaking");
            continue;
        }
        if gate == Gate::Speaking {
            if speaking {
                continue;
            }
            gate = Gate::Settling;
            quiet_since = None;
        }
        if gate == Gate::Settling {
            // Do not resume on the first quiet-looking frame: residual echo from
            // speakers, cables or looped virtual devices needs a short guard.
            // Wait for a genuinely quiet stretch before trusting the microphone.
            let rms = (block.iter().map(|v| v * v).sum::<f32>() / block.len().max(1) as f32).sqrt();
            if rms >= ECHO_QUIET {
                quiet_since = None;
            } else {
                let since = *quiet_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= Duration::from_millis(150)
                    || last_playing.is_some_and(|at| at.elapsed() > Duration::from_secs(10))
                {
                    gate = Gate::Listening;
                    drop(stream);
                    stream = session
                        .stream(&run, &stream_options)
                        .context("restart stream after speaking")?;
                    seen = 0;
                    silence_since = None;
                    status
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .input_paused = false;
                    eprintln!("engine: listening resumed");
                }
            }
            continue;
        }
        let update = stream.feed(&block).context("stream feed")?;
        let rms = (block.iter().map(|v| v * v).sum::<f32>() / block.len().max(1) as f32).sqrt();
        if rms < 0.003 {
            if silence_since.is_none() && (seen > 0 || !pending.is_empty()) {
                silence_since = Some(Instant::now());
            }
        } else {
            silence_since = None;
        }
        if update.committed_changed {
            let text = stream.text().committed;
            let new_words = complete_words(&text, &mut seen);
            for word in &new_words {
                if first_at.is_none() {
                    first_at = Some(Instant::now());
                }
                pending.push(word.clone());
            }
            update_status(status, &new_words);
            if !new_words.is_empty() {
                pending_since.get_or_insert_with(Instant::now);
            }
            // Words are spoken at a silence boundary (below). Speaking every few
            // words talks over the user, and without echo cancellation that
            // overlap feeds the app its own voice.
        }
        if silence_since.is_some_and(|at| at.elapsed() >= Duration::from_millis(800)) {
            // The stream keeps the final word in its lookahead until finalize.
            // Silence is a natural boundary: flush, speak, and start fresh.
            if !flush_stream(
                &mut stream,
                &mut seen,
                &mut pending,
                &mut first_at,
                groups,
                status,
            ) {
                break;
            }
            drop(stream);
            stream = session
                .stream(&run, &stream_options)
                .context("restart stream after silence")?;
            seen = 0;
            silence_since = None;
            pending_since = None;
        } else if pending_since.is_some_and(|at| at.elapsed() >= Duration::from_secs(10)) {
            // Uninterrupted speech: flush it rather than staying silent.
            if !flush_stream(
                &mut stream,
                &mut seen,
                &mut pending,
                &mut first_at,
                groups,
                status,
            ) {
                break;
            }
            drop(stream);
            stream = session
                .stream(&run, &stream_options)
                .context("restart stream after long speech")?;
            seen = 0;
            silence_since = None;
            pending_since = None;
        }
    }
    let _ = stream.finalize();
    let text = stream.text().full;
    let mut tail_words = complete_words(&text, &mut seen);
    tail_words.extend(text.split_whitespace().skip(seen).map(str::to_string));
    pending.extend(tail_words.iter().cloned());
    update_status(status, &tail_words);
    if !pending.is_empty() {
        let _ = groups.send(Some(Group {
            words: pending,
            at: first_at.unwrap_or_else(Instant::now),
        }));
    }
    Ok(())
}

fn complete_words(text: &str, seen: &mut usize) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let complete = if text.chars().last().is_some_and(char::is_whitespace) {
        words.len()
    } else {
        words.len().saturating_sub(1)
    };
    if complete <= *seen {
        return Vec::new();
    }
    let out = words[*seen..complete]
        .iter()
        .map(|w| (*w).to_string())
        .collect();
    *seen = complete;
    out
}

fn rest_words(text: &str, seen: &mut usize) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= *seen {
        return Vec::new();
    }
    let out = words[*seen..].iter().map(|w| (*w).to_string()).collect();
    *seen = words.len();
    out
}

fn update_status(status: &Arc<Mutex<Status>>, words: &[String]) {
    let mut status = status.lock().unwrap_or_else(|e| e.into_inner());
    if status.state == State::Failed {
        return;
    }
    if words.is_empty() {
        return;
    }
    status.transcript.push_str(&words.join(" "));
    status.transcript.push(' ');
    status.words += words.len();
}

fn tts_loop(
    config: EngineConfig,
    groups: Receiver<Option<Group>>,
    output: Arc<PcmQueue>,
    status: Arc<Mutex<Status>>,
    ready_count: Arc<AtomicUsize>,
) {
    if let Err(error) = tts_inner(&config, groups, output, &status, &ready_count) {
        fail(&status, format!("Speech: {error:#}"));
    }
}

fn tts_inner(
    config: &EngineConfig,
    groups: Receiver<Option<Group>>,
    output: Arc<PcmQueue>,
    status: &Arc<Mutex<Status>>,
    ready_count: &AtomicUsize,
) -> Result<()> {
    let checkpoint = Checkpoint::resolve(
        &config.tts_model,
        ResolveOptions {
            quant: Quant::Q80,
            weights: None,
        },
    )
    .with_context(|| format!("resolve {}", config.tts_model.display()))?;
    let mut tts = checkpoint
        .builder(Lang::En)
        .device(DeviceKind::Cpu)
        .build()
        .context("build Phonon")?;
    checkpoint.register_voices(&mut tts);
    eprintln!("engine: Phonon loaded, voices {:?}", tts.voices());
    if !tts.voices().iter().any(|name| name == &config.voice) {
        bail!(
            "voice {:?} is not installed in {}",
            config.voice,
            config.tts_model.display()
        );
    }
    let _ = tts.say("Ready.").context("warmup synthesis")?;
    let source_rate = tts.sample_rate();
    let mut resampler = Resampler::new(source_rate, config.output_rate)?;
    ready(status, ready_count);

    while let Ok(Some(group)) = groups.recv() {
        let text = group.words.join(" ");
        let mut trimmer = config
            .trim_tts
            .then(|| Trimmer::new(source_rate, config.trim_threshold, 50));
        let mut first = true;
        let mut audio_samples = 0usize;
        for chunk in tts
            .stream_with(&text, &SpeechOptions::default().voice(&config.voice))
            .with_context(|| format!("stream {text:?}"))?
        {
            let chunk = chunk.context("synthesis chunk")?;
            let pieces = match trimmer.as_mut() {
                Some(trimmer) => trimmer.feed(&chunk),
                None => vec![chunk],
            };
            for piece in pieces {
                let out = resampler.process(&piece);
                if out.is_empty() {
                    continue;
                }
                output.push(&out);
                audio_samples += out.len();
                if first {
                    first = false;
                    let delay = group.at.elapsed().as_secs_f64() * 1000.0;
                    let mut status = status.lock().unwrap_or_else(|e| e.into_inner());
                    status.group_delays.push(delay);
                    let sorted = sorted(&status.group_delays);
                    status.p50_ms = percentile(&sorted, 0.50);
                    status.p95_ms = percentile(&sorted, 0.95);
                    status.queued_seconds = output.len() as f64 / config.output_rate as f64;
                    status.dropped_seconds = output.dropped() as f64 / config.output_rate as f64;
                }
            }
        }
        if let Some(trimmer) = trimmer.as_mut() {
            for piece in trimmer.finish() {
                let out = resampler.process(&piece);
                if !out.is_empty() {
                    output.push(&out);
                    audio_samples += out.len();
                }
            }
            if !trimmer.started {
                bail!("no speech remained after boundary trimming");
            }
        }
        let _ = audio_samples;
    }
    Ok(())
}

fn sorted(values: &[f64]) -> Vec<f64> {
    let mut out = values.to_vec();
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out
}

fn percentile(sorted: &[f64], fraction: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    Some(sorted[index])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clearing_the_transcript_keeps_recognition_state() {
        let engine = Engine {
            status: Arc::new(Mutex::new(Status {
                transcript: "hello world ".into(),
                words: 2,
                ..Status::default()
            })),
            input: Mutex::new(None),
            threads: Mutex::new(Vec::new()),
        };
        engine.clear_transcript();
        assert_eq!(engine.status().transcript, "");
        assert_eq!(engine.status().words, 0);
    }

    #[test]
    fn stop_preserves_worker_failure() {
        let (tx, _rx) = sync_channel(1);
        let engine = Engine {
            status: Arc::new(Mutex::new(Status {
                state: State::Failed,
                error: Some("failed".into()),
                ..Status::default()
            })),
            input: Mutex::new(Some(tx)),
            threads: Mutex::new(Vec::new()),
        };
        engine.stop();
        assert_eq!(engine.status().state, State::Failed);
        assert_eq!(engine.status().error.as_deref(), Some("failed"));
    }
}
