//! Realtime pipeline: Parakeet Unified (transcribe.cpp/Vulkan) → Phonon (CPU)
//! → bounded PCM queue. Device playback is the caller's job.
//!
//! P0 speech continuity: one active phrase at a time, fully muted playback,
//! audio-frame endpointing, a cancellation handshake, and separated synthesis
//! and device failure domains.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use dex_audio::{PcmQueue, Resampler, Trimmer};
use ptts::checkpoint::{Checkpoint, ResolveOptions};
use ptts::preprocess::Lang;
use ptts::synth::{DeviceKind, Quant, SpeechOptions};
use serde::Serialize;

mod clock;
mod endpoint;
mod failure;
mod recognizer;
mod turn;

pub use clock::{Clock, ManualClock, RealClock};
pub use endpoint::{
    EndpointConfig, EndpointEvent, EndpointReason, FrameSplitter, PhraseClock, rms,
};
pub use failure::{
    CancelWatch, DeviceAction, DeviceState, DisconnectAction, FailureDomain, NO_PCM_CANCEL_MS,
    StopOutcome, Supervisor, SynthesisState, TurnWatch, TurnWatchAction, stop_outcome,
};
pub use recognizer::{
    FeedUpdate, FinalizeOutcome, RecogStream, Recognizer, ScriptedRecognizer, ScriptedSpec,
    ScriptedStep, ScriptedText, TextSnapshot, TranscribeRecognizer,
};
pub use turn::{Dispatch, InFlight, SendOutcome, TurnControl, TurnToken};

/// Placeholder for the playback buffer's contribution to the start-of-output
/// estimate. Real device latency is telemetry-only in P0.
pub const OUTPUT_BUFFER_MS: u64 = 50;
/// Additional quiet guard after the last observed pop.
pub const OUTPUT_GUARD_MS: u64 = 500;
/// Cancellation must be acknowledged within this window.
pub const CANCEL_ACK_MS: u64 = 1_500;
/// Bounded stop joins.
pub const STOP_DRAIN_MS: u64 = 1_500;
pub const STOP_JOIN_MS: u64 = 2_000;
pub const STOP_CONTROL_MS: u64 = 8_000;
/// Consecutive synthesis failures before the speech domain is declared failed.
pub const SYNTHESIS_FAILURE_LIMIT: u64 = 3;
/// Keep telemetry bounded for a long-running session.
const MAX_GROUP_DELAYS: usize = 240;
/// Alarm threshold for long queued output (telemetry/UI only).
const LONG_OUTPUT_SECONDS: f64 = 30.0;

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
    /// Configurable endpoint hysteresis.
    pub speech_rms: f32,
    pub quiet_rms: f32,
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
            speech_rms: 0.008,
            quiet_rms: 0.003,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Loading,
    Running,
    Stopping,
    /// A device is being reopened after one transit stall.
    Recovering,
    #[default]
    Idle,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Status {
    pub state: State,
    pub error: Option<String>,
    /// Non-fatal terminal note, currently `shutdown_incomplete`.
    pub warning: Option<String>,
    pub failure_domain: FailureDomain,
    pub synthesis_state: SynthesisState,
    pub device_state: DeviceState,
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

    // P0 counters.
    pub busy_drops: u64,
    pub dispatch_failures: u64,
    pub finalize_errors: u64,
    pub finalize_tail_loss: u64,
    pub empty_finalize: u64,
    pub degraded_finalize: u64,
    pub idle_cap_resets: u64,
    pub endpoint_silence: u64,
    pub endpoint_cap: u64,
    pub synthesis_errors: u64,
    pub tts_restarts: u64,
    pub tts_restart_failures: u64,
    pub synthesis_stalls: u64,
    pub cancel_requests: u64,
    pub cancel_timeouts: u64,
    pub device_recoveries: u64,
    pub device_recovery_failures: u64,
    pub queue_cleared_samples: u64,
    pub stop_tail_dropped: u64,
    pub shutdown_incomplete_timeouts: u64,
    pub orphan_loader_exits: u64,
    pub trimmed_empty: u64,
    pub capture_drops: u64,
    pub long_output_alarm: bool,

    /// Deduplicates a phrase's transcript append by `(generation, id)`.
    #[serde(skip)]
    pub last_transcript_key: Option<(u64, u64)>,
}

/// Destination for one completed phrase.
pub trait PhraseSink {
    fn try_send(&self, dispatch: Dispatch) -> SendOutcome;

    /// Recreates the speech worker after a disconnect. Returns false when a
    /// restart is not available; the caller then fails the synthesis domain.
    fn restart(&self) -> bool {
        false
    }
}

/// Factory that recreates the speech worker after a disconnect.
type RestartFn = Box<dyn Fn() -> Result<SyncSender<Option<Dispatch>>> + Send + Sync>;

/// Real sink: bounded dispatch through a channel of capacity one.
pub struct ChannelSink {
    tx: Mutex<Option<SyncSender<Option<Dispatch>>>>,
    restart: Option<RestartFn>,
}

impl ChannelSink {
    pub fn new(tx: SyncSender<Option<Dispatch>>) -> Self {
        Self {
            tx: Mutex::new(Some(tx)),
            restart: None,
        }
    }

    pub fn restartable(
        tx: SyncSender<Option<Dispatch>>,
        restart: impl Fn() -> Result<SyncSender<Option<Dispatch>>> + Send + Sync + 'static,
    ) -> Self {
        Self {
            tx: Mutex::new(Some(tx)),
            restart: Some(Box::new(restart)),
        }
    }

    /// Drops the sending side. A worker drains whatever is queued and then
    /// observes a disconnected channel.
    pub fn close(&self) {
        let _ = self.tx.lock().unwrap_or_else(|e| e.into_inner()).take();
    }
}

impl PhraseSink for ChannelSink {
    fn try_send(&self, dispatch: Dispatch) -> SendOutcome {
        let guard = self.tx.lock().unwrap_or_else(|e| e.into_inner());
        let Some(tx) = guard.as_ref() else {
            return SendOutcome::Disconnected;
        };
        match tx.try_send(Some(dispatch)) {
            Ok(()) => SendOutcome::Sent,
            Err(TrySendError::Full(_)) => SendOutcome::Busy,
            Err(TrySendError::Disconnected(_)) => SendOutcome::Disconnected,
        }
    }

    fn restart(&self) -> bool {
        let Some(factory) = &self.restart else {
            return false;
        };
        match factory() {
            Ok(tx) => {
                *self.tx.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
                true
            }
            Err(_) => false,
        }
    }
}

/// Input gating state used to avoid recognizing our own synthesized speech.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Gate {
    /// Feeding the recognizer normally.
    Listening,
    /// Our own audio is queued or still draining; discard microphone input.
    Speaking,
}

// ── Process-scoped loader guard ─────────────────────────────────────────────

static LOADER_ACTIVE: AtomicBool = AtomicBool::new(false);
static GENERATION_RETIRED_PENDING: AtomicBool = AtomicBool::new(false);
static ORPHAN_LOADER_EXITS: AtomicUsize = AtomicUsize::new(0);

/// Guarantees a single model loader per process. Held by the ASR thread and
/// released only when that thread actually exits.
pub struct LoaderGuard {
    _private: (),
}

impl LoaderGuard {
    pub fn acquire() -> Option<Self> {
        if GENERATION_RETIRED_PENDING.load(Ordering::SeqCst) {
            return None;
        }
        if LOADER_ACTIVE
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return None;
        }
        Some(Self { _private: () })
    }
}

impl Drop for LoaderGuard {
    fn drop(&mut self) {
        LOADER_ACTIVE.store(false, Ordering::SeqCst);
        if GENERATION_RETIRED_PENDING.swap(false, Ordering::SeqCst) {
            ORPHAN_LOADER_EXITS.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// Marks the current generation as retired while its workers finish.
pub fn retire_generation() {
    if LOADER_ACTIVE.load(Ordering::SeqCst) {
        GENERATION_RETIRED_PENDING.store(true, Ordering::SeqCst);
    }
}

pub fn loader_active() -> bool {
    LOADER_ACTIVE.load(Ordering::SeqCst)
}

pub fn generation_retired_pending() -> bool {
    GENERATION_RETIRED_PENDING.load(Ordering::SeqCst)
}

pub fn orphan_loader_exits() -> usize {
    ORPHAN_LOADER_EXITS.load(Ordering::SeqCst)
}

// ── Engine ──────────────────────────────────────────────────────────────────

pub struct Engine {
    status: Arc<Mutex<Status>>,
    input: Mutex<Option<SyncSender<Vec<f32>>>>,
    inflight: Arc<InFlight>,
    sink: Arc<ChannelSink>,
    output: Arc<PcmQueue>,
    workers: Arc<Mutex<Vec<JoinHandle<()>>>>,
    asr_thread: Mutex<Option<JoinHandle<()>>>,
    stop_thread: Mutex<Option<JoinHandle<()>>>,
    stop_requested: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
}

impl Engine {
    pub fn start(config: EngineConfig, output: Arc<PcmQueue>) -> Result<Self> {
        if config.output_rate == 0 {
            bail!("output rate must be positive");
        }
        let guard = LoaderGuard::acquire()
            .ok_or_else(|| anyhow!("a previous model loader is still exiting; retry shortly"))?;
        let status = Arc::new(Mutex::new(Status {
            state: State::Loading,
            ..Status::default()
        }));
        let (input_tx, input_rx) = sync_channel::<Vec<f32>>(200);
        let ready = Arc::new(AtomicUsize::new(0));
        let inflight = InFlight::new();
        let workers: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));
        let stopping = Arc::new(AtomicBool::new(false));
        let stop_requested = Arc::new(AtomicBool::new(false));

        let (tts_tx, tts_handle) =
            spawn_tts(&config, &output, &status, &ready).context("spawn TTS thread")?;
        workers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(tts_handle);

        let sink = {
            let config = config.clone();
            let output = Arc::clone(&output);
            let status = Arc::clone(&status);
            let ready = Arc::clone(&ready);
            let workers = Arc::clone(&workers);
            let stopping = Arc::clone(&stopping);
            Arc::new(ChannelSink::restartable(tts_tx, move || {
                let mut handles = workers.lock().unwrap_or_else(|e| e.into_inner());
                if stopping.load(Ordering::SeqCst) {
                    return Err(anyhow!("engine is stopping"));
                }
                let (tx, handle) = spawn_tts(&config, &output, &status, &ready)?;
                handles.push(handle);
                Ok(tx)
            }))
        };

        let asr_thread = {
            let config = config.clone();
            let status = Arc::clone(&status);
            let ready = Arc::clone(&ready);
            let output = Arc::clone(&output);
            let inflight = Arc::clone(&inflight);
            let sink = Arc::clone(&sink);
            let stop = Arc::clone(&stop_requested);
            thread::Builder::new()
                .name("dex-asr".into())
                .spawn(move || {
                    let _guard = guard;
                    asr_loop(
                        config, input_rx, sink, status, ready, output, inflight, stop,
                    )
                })
                .context("spawn ASR thread")?
        };

        Ok(Self {
            status,
            input: Mutex::new(Some(input_tx)),
            inflight,
            sink,
            output,
            workers,
            asr_thread: Mutex::new(Some(asr_thread)),
            stop_thread: Mutex::new(None),
            stop_requested,
            stopping,
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
            Err(TrySendError::Full(_)) => {
                self.bump(|status| status.input_drops += 1);
                false
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }

    pub fn status(&self) -> Status {
        self.lock().clone()
    }

    /// Clears the displayed transcript without disturbing recognition. The
    /// dedupe key is reset in the same critical section.
    pub fn clear_transcript(&self) {
        let mut status = self.lock();
        status.transcript.clear();
        status.words = 0;
        status.last_transcript_key = None;
    }

    /// Requests cancellation of the active phrase. Returns true when a live
    /// turn was signalled (its acknowledgement is asynchronous).
    pub fn cancel_speech(&self) -> bool {
        self.inflight.cancel_active()
    }

    /// Non-blocking stop. The caller (UI) must never join worker threads.
    pub fn request_stop(&self) {
        if self.stop_requested.swap(true, Ordering::SeqCst) {
            return;
        }
        self.stopping.store(true, Ordering::SeqCst);
        drop(self.input.lock().unwrap_or_else(|e| e.into_inner()).take());
        self.inflight.cancel_active();
        {
            let mut status = self.lock();
            if status.state != State::Failed {
                status.state = State::Stopping;
            }
        }
        let asr = self
            .asr_thread
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let workers = std::mem::take(&mut *self.workers.lock().unwrap_or_else(|e| e.into_inner()));
        let status = Arc::clone(&self.status);
        let inflight = Arc::clone(&self.inflight);
        let output = Arc::clone(&self.output);
        let sink = Arc::clone(&self.sink);
        let handle = thread::Builder::new()
            .name("dex-control".into())
            .spawn(move || stop_control(status, inflight, output, sink, asr, workers))
            .ok();
        *self.stop_thread.lock().unwrap_or_else(|e| e.into_inner()) = handle;
    }

    /// Blocking stop used by non-UI binaries. The UI calls [`request_stop`].
    /// The join is bounded to [`STOP_CONTROL_MS`] so a caller can never block
    /// forever on a worker thread.
    pub fn stop(&self) {
        self.request_stop();
        let handle = self
            .stop_thread
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(handle) = handle {
            let _ = join_with_timeout(vec![handle], Duration::from_millis(STOP_CONTROL_MS));
        }
    }

    /// Records a capture block dropped at the device callback because the
    /// feeder channel was full or closed.
    pub fn note_capture_drop(&self) {
        self.bump(|status| status.capture_drops += 1);
    }

    fn bump(&self, counter: impl FnOnce(&mut Status)) {
        let mut status = self.lock();
        counter(&mut status);
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

fn stop_control(
    status: Arc<Mutex<Status>>,
    inflight: Arc<InFlight>,
    output: Arc<PcmQueue>,
    sink: Arc<ChannelSink>,
    asr: Option<JoinHandle<()>>,
    workers: Vec<JoinHandle<()>>,
) {
    // Cancellation acknowledgement: 1.5 s, then the synthesis domain is stalled.
    let cancel_started = Instant::now();
    while inflight.is_active() && cancel_started.elapsed() < Duration::from_millis(CANCEL_ACK_MS) {
        thread::sleep(Duration::from_millis(10));
    }
    if inflight.is_active() {
        let mut st = status.lock().unwrap_or_else(|e| e.into_inner());
        st.cancel_timeouts += 1;
        st.synthesis_stalls += 1;
        st.failure_domain = FailureDomain::Synthesis;
        st.synthesis_state = SynthesisState::Stalled;
        st.error = Some("Speech: cancellation was not acknowledged".into());
        st.state = State::Failed;
    }

    // The recognizer thread exits first: it may still flush one final phrase
    // into the dispatch channel. Only then is the channel closed so the speech
    // worker drains and exits.
    let asr_joined = match asr {
        Some(handle) => join_with_timeout(vec![handle], Duration::from_millis(STOP_JOIN_MS)),
        None => true,
    };
    // A normal stop never marks the loader as orphan. Only mark it when the
    // ASR thread actually fails to exit within the bound, i.e. a true orphan.
    if !asr_joined && loader_active() {
        retire_generation();
    }
    sink.close();

    // Let the speech worker finish and the queue drain, bounded.
    let drain_started = Instant::now();
    while (inflight.is_active() || !inflight.channel_empty() || !output.is_empty())
        && drain_started.elapsed() < Duration::from_millis(STOP_DRAIN_MS)
    {
        thread::sleep(Duration::from_millis(10));
    }
    let drain_timed_out = inflight.is_active()
        || !inflight.channel_empty()
        || !output.is_empty()
        || drain_started.elapsed() >= Duration::from_millis(STOP_DRAIN_MS);

    let workers_joined = join_with_timeout(workers, Duration::from_millis(STOP_JOIN_MS));
    let joined = asr_joined && workers_joined;
    let timed_out = !joined || drain_timed_out;
    let mut st = status.lock().unwrap_or_else(|e| e.into_inner());
    if st.state != State::Failed {
        let outcome = stop_outcome(st.device_state == DeviceState::Recovering, timed_out);
        st.state = match outcome {
            StopOutcome::Recovering => State::Recovering,
            _ => State::Idle,
        };
    }
    if timed_out {
        st.warning = Some("shutdown_incomplete".into());
        st.shutdown_incomplete_timeouts += 1;
        if st.state != State::Failed {
            st.state = State::Idle;
        }
        st.orphan_loader_exits = orphan_loader_exits() as u64;
    }
}

fn join_with_timeout(handles: Vec<JoinHandle<()>>, timeout: Duration) -> bool {
    if handles.is_empty() {
        return true;
    }
    let (tx, rx) = sync_channel(1);
    let joiner = thread::Builder::new()
        .name("dex-join".into())
        .spawn(move || {
            for handle in handles {
                let _ = handle.join();
            }
            let _ = tx.send(());
        });
    match joiner {
        Ok(_) => rx.recv_timeout(timeout).is_ok(),
        Err(_) => false,
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

fn fail(status: &Arc<Mutex<Status>>, domain: FailureDomain, error: impl Into<String>) {
    let mut status = status.lock().unwrap_or_else(|e| e.into_inner());
    status.state = State::Failed;
    status.failure_domain = domain;
    status.error = Some(error.into());
    match domain {
        FailureDomain::Synthesis => status.synthesis_state = SynthesisState::Failed,
        FailureDomain::Device => status.device_state = DeviceState::Lost,
        FailureDomain::None => {}
    }
}

// ── ASR / turn loop ─────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn asr_loop(
    config: EngineConfig,
    input: Receiver<Vec<f32>>,
    sink: Arc<ChannelSink>,
    status: Arc<Mutex<Status>>,
    ready_count: Arc<AtomicUsize>,
    output: Arc<PcmQueue>,
    inflight: Arc<InFlight>,
    stop: Arc<AtomicBool>,
) {
    if let Err(error) = asr_inner(
        &config,
        input,
        sink.as_ref(),
        &status,
        &ready_count,
        &output,
        &inflight,
        &stop,
    ) {
        fail(
            &status,
            FailureDomain::None,
            format!("Recognition: {error:#}"),
        );
    }
    sink.close();
}

#[allow(clippy::too_many_arguments)]
fn asr_inner(
    config: &EngineConfig,
    input: Receiver<Vec<f32>>,
    sink: &dyn PhraseSink,
    status: &Arc<Mutex<Status>>,
    ready_count: &AtomicUsize,
    output: &PcmQueue,
    inflight: &Arc<InFlight>,
    stop: &AtomicBool,
) -> Result<()> {
    let mut recognizer = TranscribeRecognizer::load(config)?;
    ready(status, ready_count);
    let endpoint = EndpointConfig {
        speech_rms: config.speech_rms,
        quiet_rms: config.quiet_rms,
        ..EndpointConfig::default()
    };
    run_loop(
        input,
        &mut recognizer,
        sink,
        status,
        output,
        &RealClock,
        inflight,
        endpoint,
        Some(stop),
    )
}

/// True when the engine has failed and the loop must stop recognising.
fn is_failed(status: &Arc<Mutex<Status>>) -> bool {
    status.lock().unwrap_or_else(|e| e.into_inner()).state == State::Failed
}

/// Publishes a 2 s no-PCM cancellation that was never acknowledged.
fn apply_stall(status: &Arc<Mutex<Status>>, supervisor: &Supervisor, cleared: usize) {
    let mut st = status.lock().unwrap_or_else(|e| e.into_inner());
    st.cancel_timeouts += 1;
    st.synthesis_stalls += 1;
    st.failure_domain = supervisor.failure;
    st.synthesis_state = supervisor.synthesis;
    st.state = State::Failed;
    st.error = Some("Speech: cancellation was not acknowledged".into());
    st.queue_cleared_samples += cleared as u64;
}
/// work (a phrase waiting on finalize) is deliberately not output work.
pub fn can_listen(inflight: &InFlight, output: &PcmQueue, now: Instant) -> bool {
    let no_output_work = !inflight.is_active() && inflight.channel_empty() && output.is_empty();
    no_output_work
        && output.last_pop().is_none_or(|last| {
            now.saturating_duration_since(last)
                >= Duration::from_millis(OUTPUT_BUFFER_MS + OUTPUT_GUARD_MS)
        })
}

/// The recognition/turn loop. Kept free of model loading so tests can drive it
/// with a scripted recognizer and a manual clock.
#[allow(clippy::too_many_arguments)]
pub fn run_loop(
    input: Receiver<Vec<f32>>,
    recognizer: &mut dyn Recognizer,
    sink: &dyn PhraseSink,
    status: &Arc<Mutex<Status>>,
    output: &PcmQueue,
    clock: &dyn Clock,
    inflight: &Arc<InFlight>,
    endpoint: EndpointConfig,
    stop: Option<&AtomicBool>,
) -> Result<()> {
    const INPUT_RATE: usize = 16_000;
    let frame_samples = (INPUT_RATE * endpoint.frame_ms as usize / 1000).max(1);
    let mut splitter = FrameSplitter::new(frame_samples);
    let mut phrase = PhraseClock::new(endpoint);
    let mut stream = recognizer.open()?;
    let mut first_at: Option<Instant> = None;
    let mut generation = 1u64;
    let mut gate = Gate::Listening;
    let mut finalize_failures = 0u64;
    // Tracks whether the most recent endpoint produced no spoken words, so an
    // EOF on the leftover empty stream is not counted a second time.
    let mut just_finalized_empty = false;
    // Production 2 s no-PCM watchdog. It owns the `Supervisor` that decides the
    // failure domain, so there is no separate test-only cancellation path.
    let mut watch = TurnWatch::new(
        Duration::from_millis(NO_PCM_CANCEL_MS),
        Duration::from_millis(CANCEL_ACK_MS),
    );
    let mut terminal = false;

    loop {
        // A failed engine is terminal: stop recognising, never reset a zombie
        // stream, and never emit a tail from it.
        if is_failed(status) {
            terminal = true;
            break;
        }
        if stop.is_some_and(|stop| stop.load(Ordering::SeqCst)) {
            break;
        }
        let block = match input.recv_timeout(endpoint.frame_duration()) {
            Ok(block) => Some(block),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let now = clock.now();
        // The tick runs on every loop pass, including while muted: a turn that
        // produces no PCM is cancelled at 2 s, then must acknowledge within
        // 1.5 s or the synthesis domain stalls.
        match watch.tick(now, inflight.active().as_deref()) {
            TurnWatchAction::None => {}
            TurnWatchAction::Cancel { id } => {
                if inflight.cancel_turn(id) {
                    bump(status, |s| s.cancel_requests += 1);
                }
            }
            TurnWatchAction::Stalled { id } => {
                // Release the stuck turn, clear anything it queued, and publish
                // the stall. The input gate stays closed: never unmute.
                inflight.force_release(id);
                let cleared = output.clear();
                apply_stall(status, watch.supervisor(), cleared);
                terminal = true;
                break;
            }
        }
        let listening = can_listen(inflight, output, now);
        if gate == Gate::Listening && !listening {
            // Fully muted while our own voice is queued, synthesizing, or
            // inside the post-pop guard. Flush nothing: the phrase was already
            // dispatched at its endpoint.
            gate = Gate::Speaking;
            set_input_paused(status, true);
            drop(stream);
            stream = recognizer.open()?;
            phrase.reset_phrase();
            first_at = None;
            generation += 1;
            continue;
        }
        if gate == Gate::Speaking {
            if listening {
                gate = Gate::Listening;
                set_input_paused(status, false);
                drop(stream);
                stream = recognizer.open()?;
                phrase.reset_phrase();
                first_at = None;
                generation += 1;
            }
            continue;
        }

        // Re-block the capture buffer. A timeout is one silent audio frame, so
        // endpoint and cap windows keep moving when the microphone is quiet.
        let mut frames: Vec<Vec<f32>> = Vec::new();
        if let Some(block) = block {
            splitter.push(&block, |frame| frames.push(frame.to_vec()));
        } else if splitter.buffered() == 0 {
            frames.push(vec![0.0; frame_samples]);
        }

        for frame in frames {
            let update = stream.feed(&frame).context("stream feed")?;
            let _ = update;
            let snapshot = stream.text();
            let display_nonempty = !snapshot.display.trim().is_empty();
            if display_nonempty {
                just_finalized_empty = false;
            }
            if first_at.is_none() && display_nonempty {
                first_at = Some(now);
            }
            match phrase.push_frame(rms(&frame), display_nonempty) {
                EndpointEvent::Continue => {}
                EndpointEvent::UnarmedReset => {
                    bump(status, |s| s.idle_cap_resets += 1);
                    drop(stream);
                    stream = recognizer.open()?;
                    phrase.reset_phrase();
                    first_at = None;
                    generation += 1;
                }
                EndpointEvent::Endpoint(reason) => {
                    let before = stream.text();
                    let result = stream.finalize();
                    let failed = result.is_err();
                    let after = stream.text();
                    let words = emission_words(before, result, after, Some(reason), status);
                    just_finalized_empty = words.is_none();
                    if failed {
                        finalize_failures += 1;
                    } else {
                        finalize_failures = 0;
                    }
                    if let Some((words, marker)) = words {
                        let at = first_at.take().unwrap_or(now);
                        dispatch_phrase(words, at, generation, marker, status, sink, inflight, now);
                    }
                    drop(stream);
                    stream = recognizer.open()?;
                    phrase.reset_phrase();
                    first_at = None;
                    generation += 1;
                    if finalize_failures >= SYNTHESIS_FAILURE_LIMIT {
                        fail(
                            status,
                            FailureDomain::None,
                            "Recognition: finalize failed repeatedly",
                        );
                        return Ok(());
                    }
                    if !can_listen(inflight, output, now) {
                        gate = Gate::Speaking;
                        set_input_paused(status, true);
                        break;
                    }
                }
            }
        }
    }

    if terminal {
        return Ok(());
    }

    if just_finalized_empty && !phrase.is_armed() {
        return Ok(());
    }

    // EOF: finalize the last phrase. Tail dispatch is exempt from the
    // one-in-flight assertion, but it still respects the single slot: if a
    // turn is live the tail is counted and dropped rather than queued.
    let before = stream.text();
    let result = stream.finalize();
    let failed = result.is_err();
    let after = stream.text();
    if let Some((words, marker)) = emission_words(before, result, after, None, status) {
        let at = first_at.unwrap_or_else(|| clock.now());
        let free = !inflight.is_active() && inflight.channel_empty();
        if stop.is_some_and(|stop| stop.load(Ordering::SeqCst)) {
            if free {
                dispatch_stop_tail(words, at, generation, status, sink, inflight);
            } else {
                bump(status, |s| s.stop_tail_dropped += 1);
                append_transcript(status, (generation, 0), &words, Some("[tail lost]"));
            }
        } else if free {
            dispatch_phrase(words, at, generation, marker, status, sink, inflight, at);
        } else {
            bump(status, |s| s.stop_tail_dropped += 1);
            append_transcript(status, (generation, 0), &words, Some("[tail lost]"));
        }
    }
    if failed {
        finalize_failures += 1;
        if finalize_failures >= SYNTHESIS_FAILURE_LIMIT {
            fail(
                status,
                FailureDomain::None,
                "Recognition: finalize failed repeatedly",
            );
        }
    }
    Ok(())
}

/// Applies the finalize matrix and returns the words to speak plus an optional
/// transcript marker.
fn emission_words(
    before: TextSnapshot,
    result: Result<FeedUpdate>,
    after: TextSnapshot,
    reason: Option<EndpointReason>,
    status: &Arc<Mutex<Status>>,
) -> Option<(Vec<String>, Option<&'static str>)> {
    if let Some(reason) = reason {
        bump(status, |s| match reason {
            EndpointReason::Silence => s.endpoint_silence += 1,
            EndpointReason::Cap => s.endpoint_cap += 1,
        });
    }
    match result {
        Ok(update) if update.is_final => {
            // display -> full -> committed, exactly once.
            let text = if !after.display.trim().is_empty() {
                after.display
            } else if !after.full.trim().is_empty() {
                after.full
            } else {
                after.committed
            };
            let words: Vec<String> = text
                .split_whitespace()
                .filter(|word| !word.is_empty())
                .map(str::to_owned)
                .collect();
            if words.is_empty() {
                bump(status, |s| s.empty_finalize += 1);
                None
            } else {
                Some((words, None))
            }
        }
        other => {
            if other.is_err() {
                bump(status, |s| s.finalize_errors += 1);
            }
            // Never trust a rewritable or tentative suffix after a non-final
            // finalize: speak only the committed words captured before it.
            let mut words: Vec<String> = before
                .committed
                .split_whitespace()
                .filter(|word| !word.is_empty())
                .map(str::to_owned)
                .collect();
            // A committed tail without trailing whitespace is still tentative:
            // drop it so a partial token can never reach speech or transcript.
            if !words.is_empty()
                && !before.committed.trim_end().is_empty()
                && !before
                    .committed
                    .chars()
                    .last()
                    .is_some_and(char::is_whitespace)
            {
                words.pop();
                bump(status, |s| s.finalize_tail_loss += 1);
            }
            if words.is_empty() {
                bump(status, |s| s.degraded_finalize += 1);
                None
            } else {
                Some((words, Some("[partial]")))
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn dispatch_phrase(
    words: Vec<String>,
    at: Instant,
    generation: u64,
    marker: Option<&'static str>,
    status: &Arc<Mutex<Status>>,
    sink: &dyn PhraseSink,
    inflight: &Arc<InFlight>,
    now: Instant,
) {
    if words.is_empty() {
        return;
    }
    dispatch_phrase_inner(
        words, at, generation, marker, status, sink, inflight, now, true,
    );
}

#[allow(clippy::too_many_arguments)]
fn dispatch_phrase_inner(
    words: Vec<String>,
    at: Instant,
    generation: u64,
    marker: Option<&'static str>,
    status: &Arc<Mutex<Status>>,
    sink: &dyn PhraseSink,
    inflight: &Arc<InFlight>,
    now: Instant,
    allow_restart: bool,
) {
    if inflight.is_active() || !inflight.channel_empty() {
        bump(status, |s| s.busy_drops += 1);
        append_transcript(
            status,
            (generation, 0),
            &words,
            Some("[not spoken: overlapped]"),
        );
        return;
    }
    let Some(control) = inflight.begin(now) else {
        bump(status, |s| s.busy_drops += 1);
        append_transcript(
            status,
            (generation, 0),
            &words,
            Some("[not spoken: overlapped]"),
        );
        return;
    };
    let id = control.id;
    // Exactly one in-flight turn by construction; this catches a future
    // regression that reintroduces a second dispatch path.
    debug_assert_eq!(inflight.active_id(), Some(id));
    let token = TurnToken::new(Arc::clone(inflight), control);
    let retry_words = words.clone();
    match sink.try_send(Dispatch {
        id,
        words,
        at,
        token,
    }) {
        SendOutcome::Sent => {
            append_transcript(status, (generation, id), &retry_words, marker);
        }
        SendOutcome::Busy => {
            bump(status, |s| s.busy_drops += 1);
            append_transcript(
                status,
                (generation, 0),
                &retry_words,
                Some("[not spoken: overlapped]"),
            );
        }
        SendOutcome::Disconnected => {
            bump(status, |s| s.dispatch_failures += 1);
            if allow_restart && !inflight.is_active() && inflight.channel_empty() && sink.restart()
            {
                bump(status, |s| s.tts_restarts += 1);
                dispatch_phrase_inner(
                    retry_words,
                    at,
                    generation,
                    marker,
                    status,
                    sink,
                    inflight,
                    now,
                    false,
                );
            } else {
                bump(status, |s| {
                    s.tts_restart_failures += 1;
                    s.synthesis_stalls += 1;
                });
                append_transcript(
                    status,
                    (generation, 0),
                    &retry_words,
                    Some("[not spoken: synthesis unavailable]"),
                );
                fail(
                    status,
                    FailureDomain::Synthesis,
                    "Speech: synthesis worker disconnected",
                );
            }
        }
    }
}

/// Dispatches a phrase that is being flushed because the input is ending. A
/// disconnected worker is a dropped tail, never a synthesis failure or restart.
#[allow(clippy::too_many_arguments)]
fn dispatch_stop_tail(
    words: Vec<String>,
    at: Instant,
    generation: u64,
    status: &Arc<Mutex<Status>>,
    sink: &dyn PhraseSink,
    inflight: &Arc<InFlight>,
) {
    if words.is_empty() {
        return;
    }
    if inflight.is_active() || !inflight.channel_empty() {
        bump(status, |s| s.stop_tail_dropped += 1);
        append_transcript(status, (generation, 0), &words, Some("[tail lost]"));
        return;
    }
    let Some(control) = inflight.begin(at) else {
        bump(status, |s| s.stop_tail_dropped += 1);
        append_transcript(status, (generation, 0), &words, Some("[tail lost]"));
        return;
    };
    let id = control.id;
    let token = TurnToken::new(Arc::clone(inflight), control);
    let retry_words = words.clone();
    match sink.try_send(Dispatch {
        id,
        words,
        at,
        token,
    }) {
        SendOutcome::Sent => {
            append_transcript(status, (generation, id), &retry_words, None);
        }
        SendOutcome::Busy | SendOutcome::Disconnected => {
            inflight.release_failed(id);
            bump(status, |s| s.stop_tail_dropped += 1);
            append_transcript(status, (generation, 0), &retry_words, Some("[tail lost]"));
        }
    }
}

fn append_transcript(
    status: &Arc<Mutex<Status>>,
    key: (u64, u64),
    words: &[String],
    marker: Option<&str>,
) {
    let mut status = status.lock().unwrap_or_else(|e| e.into_inner());
    if status.state == State::Failed && marker.is_none() {
        return;
    }
    if status.last_transcript_key == Some(key) {
        return;
    }
    status.last_transcript_key = Some(key);
    if !words.is_empty() {
        if !status.transcript.is_empty() && !status.transcript.ends_with(' ') {
            status.transcript.push(' ');
        }
        status.transcript.push_str(&words.join(" "));
        status.words += words.len();
        status.transcript.push(' ');
    }
    if let Some(marker) = marker {
        if !status.transcript.is_empty() && !status.transcript.ends_with(' ') {
            status.transcript.push(' ');
        }
        status.transcript.push_str(marker);
        status.transcript.push(' ');
    }
}

fn set_input_paused(status: &Arc<Mutex<Status>>, paused: bool) {
    status
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .input_paused = paused;
}

fn bump(status: &Arc<Mutex<Status>>, counter: impl FnOnce(&mut Status)) {
    let mut status = status.lock().unwrap_or_else(|e| e.into_inner());
    counter(&mut status);
}

// ── TTS worker ──────────────────────────────────────────────────────────────

fn spawn_tts(
    config: &EngineConfig,
    output: &Arc<PcmQueue>,
    status: &Arc<Mutex<Status>>,
    ready: &Arc<AtomicUsize>,
) -> Result<(SyncSender<Option<Dispatch>>, JoinHandle<()>)> {
    let (tx, rx) = sync_channel::<Option<Dispatch>>(1);
    let config = config.clone();
    let output = Arc::clone(output);
    let status = Arc::clone(status);
    let ready = Arc::clone(ready);
    let handle = thread::Builder::new()
        .name("dex-tts".into())
        .spawn(move || tts_loop(config, rx, output, status, ready))
        .context("spawn TTS thread")?;
    Ok((tx, handle))
}

fn tts_loop(
    config: EngineConfig,
    groups: Receiver<Option<Dispatch>>,
    output: Arc<PcmQueue>,
    status: Arc<Mutex<Status>>,
    ready_count: Arc<AtomicUsize>,
) {
    if let Err(error) = tts_inner(&config, groups, output, &status, &ready_count) {
        fail(
            &status,
            FailureDomain::Synthesis,
            format!("Speech: {error:#}"),
        );
    }
}

fn tts_inner(
    config: &EngineConfig,
    groups: Receiver<Option<Dispatch>>,
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

    while let Ok(Some(dispatch)) = groups.recv() {
        dispatch.token.received();
        set_synthesis(status, SynthesisState::Synthesizing);
        if dispatch.token.cancel_requested() {
            finish_cancelled(status, &output, &dispatch);
            continue;
        }
        let text = dispatch.words.join(" ");
        let at = dispatch.at;
        let mut trimmer = config
            .trim_tts
            .then(|| Trimmer::new(source_rate, config.trim_threshold, 50));
        let mut audio_samples = 0usize;
        let mut cancelled = false;

        let result = (|| -> Result<()> {
            for chunk in tts
                .stream_with(&text, &SpeechOptions::default().voice(&config.voice))
                .with_context(|| format!("stream {text:?}"))?
            {
                if dispatch.token.cancel_requested() {
                    cancelled = true;
                    break;
                }
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
                    if dispatch.token.cancel_requested() {
                        cancelled = true;
                        break;
                    }
                    output.push(&out);
                    dispatch.token.mark_pcm();
                    audio_samples += out.len();
                }
                if cancelled {
                    break;
                }
            }
            if !cancelled && let Some(trimmer) = trimmer.as_mut() {
                for piece in trimmer.finish() {
                    if dispatch.token.cancel_requested() {
                        cancelled = true;
                        break;
                    }
                    let out = resampler.process(&piece);
                    if !out.is_empty() {
                        output.push(&out);
                        dispatch.token.mark_pcm();
                        audio_samples += out.len();
                    }
                }
                if !trimmer.started {
                    bump(status, |s| s.trimmed_empty += 1);
                }
            }
            Ok(())
        })();

        if cancelled {
            finish_cancelled(status, &output, &dispatch);
            continue;
        }
        // Close the race where cancellation arrives after the last push but
        // before the worker would otherwise release the turn.
        if dispatch.token.cancel_requested() {
            finish_cancelled(status, &output, &dispatch);
            continue;
        }
        if let Err(error) = result {
            bump(status, |s| s.synthesis_errors += 1);
            append_marker(status, "[speech error]");
            let failures = status
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .synthesis_errors;
            if failures >= SYNTHESIS_FAILURE_LIMIT {
                fail(
                    status,
                    FailureDomain::Synthesis,
                    format!("Speech: {error:#}"),
                );
                return Ok(());
            }
            set_synthesis(status, SynthesisState::Idle);
            continue;
        }

        if audio_samples > 0 {
            update_output_status(status, &output, config.output_rate, at);
        }
        set_synthesis(status, SynthesisState::Idle);
        // `dispatch` drops here; the token releases the single in-flight slot.
    }
    Ok(())
}

fn finish_cancelled(status: &Arc<Mutex<Status>>, output: &PcmQueue, dispatch: &Dispatch) {
    let cleared = output.clear();
    if dispatch.token.has_pcm() {
        append_marker(status, "[partial]");
    }
    bump(status, |s| {
        s.queue_cleared_samples += cleared as u64;
    });
    set_synthesis(status, SynthesisState::Idle);
}

fn append_marker(status: &Arc<Mutex<Status>>, marker: &str) {
    let mut status = status.lock().unwrap_or_else(|e| e.into_inner());
    if !status.transcript.is_empty() && !status.transcript.ends_with(' ') {
        status.transcript.push(' ');
    }
    status.transcript.push_str(marker);
    status.transcript.push(' ');
}

fn set_synthesis(status: &Arc<Mutex<Status>>, state: SynthesisState) {
    let mut status = status.lock().unwrap_or_else(|e| e.into_inner());
    if status.failure_domain != FailureDomain::Synthesis {
        status.synthesis_state = state;
    }
}

fn update_output_status(status: &Arc<Mutex<Status>>, output: &PcmQueue, rate: u32, at: Instant) {
    let delay = at.elapsed().as_secs_f64() * 1000.0;
    let mut status = status.lock().unwrap_or_else(|e| e.into_inner());
    status.group_delays.push(delay);
    if status.group_delays.len() > MAX_GROUP_DELAYS {
        let excess = status.group_delays.len() - MAX_GROUP_DELAYS;
        status.group_delays.drain(..excess);
    }
    let sorted = sorted(&status.group_delays);
    status.p50_ms = percentile(&sorted, 0.50);
    status.p95_ms = percentile(&sorted, 0.95);
    status.queued_seconds = output.len() as f64 / rate.max(1) as f64;
    status.dropped_seconds = output.dropped() as f64 / rate.max(1) as f64;
    status.long_output_alarm = status.queued_seconds >= LONG_OUTPUT_SECONDS;
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
    use std::sync::mpsc::sync_channel;

    fn test_engine(status: Status) -> Engine {
        let (tx, _rx) = sync_channel::<Option<Dispatch>>(1);
        Engine {
            status: Arc::new(Mutex::new(status)),
            input: Mutex::new(Some(sync_channel(1).0)),
            inflight: InFlight::new(),
            sink: Arc::new(ChannelSink::new(tx)),
            output: Arc::new(PcmQueue::new(16)),
            workers: Arc::new(Mutex::new(Vec::new())),
            asr_thread: Mutex::new(None),
            stop_thread: Mutex::new(None),
            stop_requested: Arc::new(AtomicBool::new(false)),
            stopping: Arc::new(AtomicBool::new(false)),
        }
    }

    #[test]
    fn clearing_the_transcript_keeps_recognition_state() {
        let engine = test_engine(Status {
            transcript: "hello world ".into(),
            words: 2,
            last_transcript_key: Some((3, 9)),
            ..Status::default()
        });
        engine.clear_transcript();
        assert_eq!(engine.status().transcript, "");
        assert_eq!(engine.status().words, 0);
        assert_eq!(engine.status().last_transcript_key, None);
    }

    #[test]
    fn stop_preserves_worker_failure() {
        let engine = test_engine(Status {
            state: State::Failed,
            error: Some("failed".into()),
            ..Status::default()
        });
        engine.stop();
        assert_eq!(engine.status().state, State::Failed);
        assert_eq!(engine.status().error.as_deref(), Some("failed"));
    }

    #[test]
    fn request_stop_is_non_blocking_and_reaches_a_terminal_state() {
        let engine = test_engine(Status::default());
        let started = Instant::now();
        engine.request_stop();
        assert!(started.elapsed() < Duration::from_millis(100));
        let deadline = Instant::now() + Duration::from_secs(2);
        while engine.status().state == State::Stopping && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(engine.status().state, State::Idle);
    }

    #[test]
    fn loader_guard_refuses_a_second_loader_until_the_orphan_exits() {
        let _lock = LOADER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Serialise against other tests that may touch the process-scoped flag.
        let guard = LoaderGuard::acquire().expect("first loader");
        assert!(LoaderGuard::acquire().is_none());
        retire_generation();
        assert!(generation_retired_pending());
        drop(guard);
        assert!(!loader_active());
        assert!(!generation_retired_pending());
        assert!(LoaderGuard::acquire().is_some());
    }

    #[test]
    fn liveness_requires_an_empty_queue_and_the_post_pop_guard() {
        let inflight = InFlight::new();
        let output = PcmQueue::new(8);
        let now = Instant::now();
        assert!(can_listen(&inflight, &output, now));
        output.push(&[1.0; 4]);
        assert!(!can_listen(&inflight, &output, now));
        let mut block = [0.0; 4];
        output.pop_into_at(&mut block, now);
        assert!(!can_listen(
            &inflight,
            &output,
            now + Duration::from_millis(549)
        ));
        assert!(can_listen(
            &inflight,
            &output,
            now + Duration::from_millis(550)
        ));
    }

    #[test]
    fn transcript_is_appended_once_per_generation_and_id() {
        let status = Arc::new(Mutex::new(Status::default()));
        let words = vec!["hello".to_owned()];
        append_transcript(&status, (2, 7), &words, None);
        append_transcript(&status, (2, 7), &words, None);
        assert_eq!(status.lock().unwrap().transcript, "hello ");
        assert_eq!(status.lock().unwrap().words, 1);
        append_transcript(&status, (3, 8), &words, None);
        assert_eq!(status.lock().unwrap().transcript, "hello hello ");
    }

    #[test]
    fn cancellation_clears_queued_pcm_and_marks_partial() {
        let inflight = InFlight::new();
        let control = inflight.begin(Instant::now()).unwrap();
        let token = TurnToken::new(Arc::clone(&inflight), control);
        token.received();
        token.mark_pcm();
        let output = PcmQueue::new(16);
        output.push(&[1.0; 6]);
        let dispatch = Dispatch {
            id: token.id(),
            words: vec!["hello".to_owned()],
            at: Instant::now(),
            token,
        };
        assert!(inflight.cancel_active());
        let status = Arc::new(Mutex::new(Status::default()));
        finish_cancelled(&status, &output, &dispatch);

        assert!(output.is_empty(), "no cancelled PCM may remain queued");
        let status = status.lock().unwrap();
        assert_eq!(status.queue_cleared_samples, 6);
        assert!(status.transcript.contains("[partial]"));
        assert_eq!(status.synthesis_state, SynthesisState::Idle);
    }

    struct PanicSink;

    impl PhraseSink for PanicSink {
        fn try_send(&self, _dispatch: Dispatch) -> SendOutcome {
            panic!("busy phrase must not reach the sink");
        }
    }

    #[test]
    fn a_second_phrase_is_counted_busy_not_dispatched() {
        let inflight = InFlight::new();
        let _held = inflight.begin(Instant::now()).expect("held turn");
        let status = Arc::new(Mutex::new(Status::default()));
        dispatch_phrase(
            vec!["second".to_owned()],
            Instant::now(),
            2,
            None,
            &status,
            &PanicSink,
            &inflight,
            Instant::now(),
        );
        let status = status.lock().unwrap();
        assert_eq!(status.busy_drops, 1);
        assert!(status.transcript.contains("[not spoken: overlapped]"));
    }

    #[test]
    fn finalize_matrix_prefers_display_then_full_then_committed() {
        let status = Arc::new(Mutex::new(Status::default()));
        let before = TextSnapshot::default();
        let after = TextSnapshot {
            display: "final words".into(),
            full: "final words".into(),
            committed: "final".into(),
            ..TextSnapshot::default()
        };
        let event = emission_words(
            before,
            Ok(FeedUpdate {
                is_final: true,
                ..FeedUpdate::default()
            }),
            after,
            Some(EndpointReason::Silence),
            &status,
        );
        assert_eq!(
            event,
            Some((vec!["final".to_owned(), "words".to_owned()], None))
        );
        assert_eq!(status.lock().unwrap().endpoint_silence, 1);
    }

    #[test]
    fn capture_drops_are_counted() {
        let engine = test_engine(Status::default());
        engine.note_capture_drop();
        engine.note_capture_drop();
        assert_eq!(engine.status().capture_drops, 2);
    }

    #[test]
    fn join_with_timeout_bounds_close() {
        let handle = thread::spawn(|| thread::sleep(Duration::from_secs(5)));
        let started = Instant::now();
        let joined = join_with_timeout(vec![handle], Duration::from_millis(50));
        assert!(!joined);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    static LOADER_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn normal_stop_does_not_mark_the_loader_as_orphan() {
        let _lock = LOADER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let before = orphan_loader_exits();
        let engine = test_engine(Status::default());
        engine.stop();
        assert!(!generation_retired_pending());
        assert_eq!(orphan_loader_exits(), before);
        let guard = LoaderGuard::acquire().expect("loader free after normal stop");
        drop(guard);
    }

    #[test]
    fn orphan_exit_is_counted_only_after_the_loader_actually_exits() {
        let _lock = LOADER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let before = orphan_loader_exits();
        let guard = LoaderGuard::acquire().expect("first loader");
        retire_generation();
        assert!(generation_retired_pending());
        assert_eq!(orphan_loader_exits(), before);
        drop(guard);
        assert_eq!(orphan_loader_exits(), before + 1);
        assert!(!generation_retired_pending());
    }

    #[test]
    fn metric_delay_is_computed_from_at() {
        let status = Arc::new(Mutex::new(Status::default()));
        let output = PcmQueue::new(16);
        output.push(&[0.1; 4]);
        let at = Instant::now();
        update_output_status(&status, &output, 16_000, at);
        let s = status.lock().unwrap();
        assert!(s.p50_ms.is_some());
        assert!(s.p95_ms.is_some());
        assert_eq!(s.group_delays.len(), 1);
    }
}
