//! Hermetic behavioural tests for P0 speech continuity.
//!
//! They drive the production `run_loop` with a scripted recognizer, a
//! deterministic clock and a sink: no model, GPU or microphone is involved.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use dex_audio::PcmQueue;
use dex_core::{
    Clock, Dispatch, EndpointConfig, FailureDomain, FinalizeOutcome, InFlight, PhraseSink,
    RealClock, ScriptedRecognizer, ScriptedSpec, ScriptedStep, ScriptedText, SendOutcome, Status,
    SynthesisState, run_loop,
};

/// Advances 20 ms on every `now()` call. The loop reads the clock at most a few
/// times per 20 ms frame, so time only ever runs faster than the audio clock.
struct AutoClock {
    base: Instant,
    ticks: AtomicU64,
}

impl AutoClock {
    fn new() -> Self {
        Self {
            base: Instant::now(),
            ticks: AtomicU64::new(0),
        }
    }
}

impl Clock for AutoClock {
    fn now(&self) -> Instant {
        self.base + Duration::from_millis(20 * self.ticks.fetch_add(1, Ordering::SeqCst))
    }
}

#[derive(Default)]
struct RecordingSink {
    groups: Mutex<Vec<Vec<String>>>,
}

impl PhraseSink for RecordingSink {
    fn try_send(&self, dispatch: Dispatch) -> SendOutcome {
        self.groups.lock().unwrap().push(dispatch.words);
        SendOutcome::Sent
    }
}

/// Holds the dispatch (and therefore the single turn slot) until the test
/// releases it. Used to model a speech worker that has not produced PCM.
#[derive(Default)]
struct HoldSink {
    groups: Mutex<Vec<Vec<String>>>,
    held: Mutex<Vec<Dispatch>>,
}

impl PhraseSink for HoldSink {
    fn try_send(&self, dispatch: Dispatch) -> SendOutcome {
        self.groups.lock().unwrap().push(dispatch.words.clone());
        self.held.lock().unwrap().push(dispatch);
        SendOutcome::Sent
    }
}

impl HoldSink {
    fn words(&self) -> Vec<String> {
        self.groups
            .lock()
            .unwrap()
            .iter()
            .map(|words| words.join(" "))
            .collect()
    }

    fn release(&self) {
        self.held.lock().unwrap().clear();
    }

    fn held(&self) -> usize {
        self.held.lock().unwrap().len()
    }
}

/// Drops the dispatch as soon as cancellation is requested, modelling a worker
/// that acknowledges the handshake.
#[derive(Default)]
struct AckOnCancelSink {
    groups: Mutex<Vec<String>>,
}

impl PhraseSink for AckOnCancelSink {
    fn try_send(&self, dispatch: Dispatch) -> SendOutcome {
        self.groups.lock().unwrap().push(dispatch.words.join(" "));
        thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(6);
            while !dispatch.token.cancel_requested() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(1));
            }
            drop(dispatch);
        });
        SendOutcome::Sent
    }
}

struct DisconnectedSink;

impl PhraseSink for DisconnectedSink {
    fn try_send(&self, _dispatch: Dispatch) -> SendOutcome {
        SendOutcome::Disconnected
    }
}

/// One phrase whose committed text appears after a single frame.
fn spec(committed: &str, finalize: FinalizeOutcome) -> ScriptedSpec {
    ScriptedSpec {
        steps: vec![ScriptedStep {
            after_frames: 1,
            text: ScriptedText::new(committed, ""),
        }],
        finalize,
        feed_error_at: None,
    }
}

type LoopHandle = (
    SyncSender<Vec<f32>>,
    Arc<PcmQueue>,
    Arc<InFlight>,
    Arc<Mutex<Status>>,
    thread::JoinHandle<()>,
);

fn spawn_run(
    specs: Vec<ScriptedSpec>,
    sink: Arc<dyn PhraseSink + Send + Sync>,
    clock: Arc<dyn Clock>,
    initial: Status,
    stop: Option<Arc<AtomicBool>>,
) -> LoopHandle {
    let (tx, rx) = sync_channel::<Vec<f32>>(4096);
    let mut recognizer = ScriptedRecognizer::new(specs);
    let status = Arc::new(Mutex::new(initial));
    let output = Arc::new(PcmQueue::new(4096));
    let inflight = InFlight::new();
    let status_thread = Arc::clone(&status);
    let output_thread = Arc::clone(&output);
    let inflight_thread = Arc::clone(&inflight);
    let handle = thread::spawn(move || {
        run_loop(
            rx,
            &mut recognizer,
            sink.as_ref(),
            &status_thread,
            &output_thread,
            clock.as_ref(),
            &inflight_thread,
            EndpointConfig::default(),
            stop.as_deref(),
        )
        .unwrap();
    });
    (tx, output, inflight, status, handle)
}

fn paced_feeder(
    tx: &SyncSender<Vec<f32>>,
    level: f32,
    run: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    let tx = tx.clone();
    thread::spawn(move || {
        while run.load(Ordering::SeqCst) {
            if tx.send(vec![level; 320]).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
    })
}

fn wait_until(limit: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(5));
    }
    condition()
}

#[test]
fn one_word_phrase_flushes_after_800ms_of_silence() {
    let specs = vec![ScriptedSpec {
        steps: vec![ScriptedStep {
            after_frames: 1,
            text: ScriptedText::new("hello", ""),
        }],
        finalize: FinalizeOutcome::Final(ScriptedText::new("hello", "")),
        feed_error_at: None,
    }];
    let sink = Arc::new(RecordingSink::default());
    let (tx, _output, _inflight, status, handle) = spawn_run(
        specs,
        Arc::clone(&sink) as Arc<dyn PhraseSink + Send + Sync>,
        Arc::new(AutoClock::new()),
        Status::default(),
        None,
    );

    tx.send(vec![0.5; 320]).unwrap();
    for _ in 0..100 {
        tx.send(vec![0.0; 320]).unwrap();
    }
    let flushed = wait_until(Duration::from_millis(500), || {
        !sink.groups.lock().unwrap().is_empty()
    });
    drop(tx);
    handle.join().unwrap();

    assert!(
        flushed,
        "a one-word phrase must be dispatched after 800 ms of audio silence, before input ends"
    );
    assert!(status.lock().unwrap().endpoint_silence >= 1);
}

#[test]
fn display_text_in_the_hysteresis_band_arms_the_cap_and_mutes() {
    // 0.005 is below the speech threshold (0.008) and above the silence
    // threshold (0.003): only the per-phrase display text can arm this phrase.
    let specs = vec![scripted_display("hello")];
    let sink = Arc::new(HoldSink::default());
    let (tx, _output, inflight, status, handle) = spawn_run(
        specs,
        Arc::clone(&sink) as Arc<dyn PhraseSink + Send + Sync>,
        Arc::new(AutoClock::new()),
        Status::default(),
        None,
    );

    for _ in 0..600 {
        tx.send(vec![0.005; 320]).unwrap();
    }
    drop(tx);
    handle.join().unwrap();

    assert_eq!(sink.words(), vec!["hello".to_owned()]);
    let status = status.lock().unwrap();
    assert_eq!(status.endpoint_cap, 1, "the anchored 10 s cap must fire");
    assert_eq!(status.endpoint_silence, 0, "the band is not silence");
    assert!(
        status.input_paused,
        "the cap dispatch must hold the input gate while the turn is live"
    );
    drop(status);
    assert_eq!(sink.held(), 1);
    inflight.cancel_active();
    sink.release();
}

fn scripted_display(text: &str) -> ScriptedSpec {
    ScriptedSpec {
        steps: vec![ScriptedStep {
            after_frames: 1,
            text: ScriptedText::new("", text),
        }],
        finalize: FinalizeOutcome::Final(ScriptedText::new("", text)),
        feed_error_at: None,
    }
}

#[test]
fn not_final_finalize_emits_only_pre_finalize_committed_words() {
    let specs = vec![spec(
        "hello ",
        FinalizeOutcome::NotFinal(ScriptedText::new("hello ", "world")),
    )];
    let sink = Arc::new(RecordingSink::default());
    let (tx, _output, _inflight, _status, handle) = spawn_run(
        specs,
        Arc::clone(&sink) as Arc<dyn PhraseSink + Send + Sync>,
        Arc::new(AutoClock::new()),
        Status::default(),
        None,
    );

    tx.send(vec![0.5; 320]).unwrap();
    for _ in 0..200 {
        tx.send(vec![0.0; 320]).unwrap();
    }
    drop(tx);
    handle.join().unwrap();

    assert_eq!(
        sink.groups.lock().unwrap().clone(),
        vec![vec!["hello".to_owned()]],
        "a non-final finalize must not smuggle the tentative word into speech"
    );
}

#[test]
fn error_finalize_is_non_fatal_and_emits_committed_words() {
    let specs = vec![spec("hello ", FinalizeOutcome::Error)];
    let sink = Arc::new(RecordingSink::default());
    let (tx, _output, _inflight, status, handle) = spawn_run(
        specs,
        Arc::clone(&sink) as Arc<dyn PhraseSink + Send + Sync>,
        Arc::new(AutoClock::new()),
        Status::default(),
        None,
    );

    tx.send(vec![0.5; 320]).unwrap();
    for _ in 0..60 {
        tx.send(vec![0.0; 320]).unwrap();
    }
    drop(tx);
    handle.join().unwrap();

    assert_eq!(
        sink.groups.lock().unwrap().clone(),
        vec![vec!["hello".to_owned()]]
    );
    assert_eq!(status.lock().unwrap().finalize_errors, 1);
}

#[test]
fn degraded_finalize_drops_an_incomplete_tail_token() {
    // "hello wor" has no trailing whitespace: "wor" is still tentative and
    // must never reach the speech worker or the transcript.
    let specs = vec![spec("hello wor", FinalizeOutcome::Error)];
    let sink = Arc::new(RecordingSink::default());
    let (tx, _output, _inflight, status, handle) = spawn_run(
        specs,
        Arc::clone(&sink) as Arc<dyn PhraseSink + Send + Sync>,
        Arc::new(AutoClock::new()),
        Status::default(),
        None,
    );

    tx.send(vec![0.5; 320]).unwrap();
    for _ in 0..60 {
        tx.send(vec![0.0; 320]).unwrap();
    }
    drop(tx);
    handle.join().unwrap();

    assert_eq!(
        sink.groups.lock().unwrap().clone(),
        vec![vec!["hello".to_owned()]],
        "only complete committed words may be spoken"
    );
    let status = status.lock().unwrap();
    assert_eq!(status.finalize_tail_loss, 1);
    assert_eq!(status.finalize_errors, 1);
    assert!(!status.transcript.contains("wor"));
}

#[test]
fn successful_empty_finalize_never_emits_the_pre_snapshot() {
    let specs = vec![ScriptedSpec {
        steps: vec![ScriptedStep {
            after_frames: 1,
            text: ScriptedText::new("hello", ""),
        }],
        finalize: FinalizeOutcome::Final(ScriptedText::default()),
        feed_error_at: None,
    }];
    let sink = Arc::new(RecordingSink::default());
    let (tx, _output, _inflight, status, handle) = spawn_run(
        specs,
        Arc::clone(&sink) as Arc<dyn PhraseSink + Send + Sync>,
        Arc::new(AutoClock::new()),
        Status::default(),
        None,
    );

    tx.send(vec![0.5; 320]).unwrap();
    for _ in 0..60 {
        tx.send(vec![0.0; 320]).unwrap();
    }
    drop(tx);
    handle.join().unwrap();

    assert!(sink.groups.lock().unwrap().is_empty());
    let status = status.lock().unwrap();
    assert_eq!(status.empty_finalize, 1);
    assert!(status.transcript.is_empty());
}

#[test]
fn an_empty_finalize_emits_no_phrase() {
    let specs = vec![ScriptedSpec {
        steps: Vec::new(),
        finalize: FinalizeOutcome::Final(ScriptedText::default()),
        feed_error_at: None,
    }];
    let sink = Arc::new(RecordingSink::default());
    let (tx, _output, _inflight, status, handle) = spawn_run(
        specs,
        Arc::clone(&sink) as Arc<dyn PhraseSink + Send + Sync>,
        Arc::new(AutoClock::new()),
        Status::default(),
        None,
    );

    for _ in 0..600 {
        tx.send(vec![0.0; 320]).unwrap();
    }
    drop(tx);
    handle.join().unwrap();

    assert!(sink.groups.lock().unwrap().is_empty());
    assert_eq!(status.lock().unwrap().empty_finalize, 1);
    assert_eq!(status.lock().unwrap().idle_cap_resets, 1);
}

#[test]
fn unarmed_phrase_resets_without_finalize_or_emit() {
    let specs = vec![ScriptedSpec {
        steps: Vec::new(),
        finalize: FinalizeOutcome::Final(ScriptedText::default()),
        feed_error_at: None,
    }];
    let sink = Arc::new(RecordingSink::default());
    let (tx, _output, _inflight, status, handle) = spawn_run(
        specs,
        Arc::clone(&sink) as Arc<dyn PhraseSink + Send + Sync>,
        Arc::new(AutoClock::new()),
        Status::default(),
        None,
    );

    for _ in 0..600 {
        tx.send(vec![0.0; 320]).unwrap();
    }
    drop(tx);
    handle.join().unwrap();

    assert!(sink.groups.lock().unwrap().is_empty());
    let status = status.lock().unwrap();
    assert_eq!(status.idle_cap_resets, 1);
    assert_eq!(status.endpoint_cap, 0);
    assert_eq!(status.endpoint_silence, 0);
}

#[test]
fn failed_state_stops_the_loop_without_resetting_or_emitting() {
    let specs = vec![ScriptedSpec {
        steps: Vec::new(),
        finalize: FinalizeOutcome::Final(ScriptedText::new("hello ", "")),
        feed_error_at: None,
    }];
    let sink = Arc::new(RecordingSink::default());
    let (tx, _output, _inflight, status, handle) = spawn_run(
        specs,
        Arc::clone(&sink) as Arc<dyn PhraseSink + Send + Sync>,
        Arc::new(AutoClock::new()),
        Status {
            state: dex_core::State::Failed,
            ..Status::default()
        },
        None,
    );

    for _ in 0..600 {
        let _ = tx.send(vec![0.0; 320]);
    }
    drop(tx);
    handle.join().unwrap();

    assert!(sink.groups.lock().unwrap().is_empty());
    let status = status.lock().unwrap();
    assert_eq!(status.state, dex_core::State::Failed);
    assert_eq!(
        status.idle_cap_resets, 0,
        "a failed engine must not reset a zombie stream"
    );
    assert_eq!(status.empty_finalize, 0);
}

#[test]
fn stopping_tail_is_audible_when_the_sink_is_connected() {
    let specs = vec![ScriptedSpec {
        steps: Vec::new(),
        finalize: FinalizeOutcome::Final(ScriptedText::new("bye ", "")),
        feed_error_at: None,
    }];
    let sink = Arc::new(RecordingSink::default());
    let stop = Arc::new(AtomicBool::new(true));
    let (tx, _output, _inflight, status, handle) = spawn_run(
        specs,
        Arc::clone(&sink) as Arc<dyn PhraseSink + Send + Sync>,
        Arc::new(AutoClock::new()),
        Status::default(),
        Some(stop),
    );
    drop(tx);
    handle.join().unwrap();

    assert_eq!(
        sink.groups.lock().unwrap().clone(),
        vec![vec!["bye".to_owned()]]
    );
    let status = status.lock().unwrap();
    assert_eq!(status.stop_tail_dropped, 0);
    assert_eq!(status.dispatch_failures, 0);
    assert_ne!(status.state, dex_core::State::Failed);
}

#[test]
fn stopping_tail_disconnect_is_counted_as_dropped_not_failed() {
    let specs = vec![ScriptedSpec {
        steps: Vec::new(),
        finalize: FinalizeOutcome::Final(ScriptedText::new("bye ", "")),
        feed_error_at: None,
    }];
    let sink = Arc::new(DisconnectedSink);
    let stop = Arc::new(AtomicBool::new(true));
    let (tx, _output, _inflight, status, handle) = spawn_run(
        specs,
        sink as Arc<dyn PhraseSink + Send + Sync>,
        Arc::new(AutoClock::new()),
        Status::default(),
        Some(stop),
    );
    drop(tx);
    handle.join().unwrap();

    let status = status.lock().unwrap();
    assert_eq!(status.stop_tail_dropped, 1);
    assert_eq!(status.dispatch_failures, 0, "a stop tail is not a failure");
    assert_eq!(status.synthesis_stalls, 0);
    assert_ne!(status.state, dex_core::State::Failed);
}

#[test]
fn no_pcm_turn_cancels_at_two_seconds_and_the_gate_resumes() {
    // spec 0: first phrase. spec 1: opened when the gate closes.
    // spec 2: opened when the gate reopens, proving listening resumes.
    let specs = vec![
        spec(
            "hello ",
            FinalizeOutcome::Final(ScriptedText::new("hello ", "")),
        ),
        ScriptedSpec::default(),
        spec(
            "again ",
            FinalizeOutcome::Final(ScriptedText::new("again ", "")),
        ),
    ];
    let sink = Arc::new(AckOnCancelSink::default());
    let (tx, _output, inflight, status, handle) = spawn_run(
        specs,
        Arc::clone(&sink) as Arc<dyn PhraseSink + Send + Sync>,
        Arc::new(RealClock),
        Status::default(),
        None,
    );
    let feed = Arc::new(AtomicBool::new(true));
    let feeder = paced_feeder(&tx, 0.0, Arc::clone(&feed));

    let first = wait_until(Duration::from_secs(3), || {
        !sink.groups.lock().unwrap().is_empty()
    });
    let muted = wait_until(Duration::from_secs(1), || {
        status.lock().unwrap().input_paused
    });
    let cancelled = wait_until(Duration::from_secs(5), || {
        status.lock().unwrap().cancel_requests >= 1
    });
    let resumed = wait_until(Duration::from_secs(4), || {
        !status.lock().unwrap().input_paused
    });
    let second = wait_until(Duration::from_secs(4), || {
        sink.groups.lock().unwrap().len() >= 2
    });

    feed.store(false, Ordering::SeqCst);
    drop(tx);
    inflight.cancel_active();
    handle.join().unwrap();
    feeder.join().unwrap();

    assert!(first, "the first phrase must be dispatched");
    assert!(muted, "the input gate must close while the turn is live");
    assert!(cancelled, "a turn with no PCM must be cancelled at 2 s");
    assert!(
        resumed,
        "the gate must reopen after the cancellation is acked"
    );
    assert!(second, "a phrase received after resume must be dispatched");
    assert_eq!(
        sink.groups.lock().unwrap().clone(),
        vec!["hello".to_owned(), "again".to_owned()]
    );
    assert_eq!(status.lock().unwrap().cancel_timeouts, 0);
}

#[test]
fn gate_resumes_after_cancel_even_when_capture_stalls() {
    // Feed a burst that produces the phrase, then stop feeding entirely: the
    // loop's own recv_timeout ticks must still drive the no-PCM cancel and the
    // gate must reopen once the worker acknowledges.
    let specs = vec![
        spec(
            "hello ",
            FinalizeOutcome::Final(ScriptedText::new("hello ", "")),
        ),
        ScriptedSpec::default(),
        ScriptedSpec::default(),
    ];
    let sink = Arc::new(AckOnCancelSink::default());
    let (tx, _output, _inflight, status, handle) = spawn_run(
        specs,
        Arc::clone(&sink) as Arc<dyn PhraseSink + Send + Sync>,
        Arc::new(RealClock),
        Status::default(),
        None,
    );

    tx.send(vec![0.5; 320 * 2]).unwrap();
    for _ in 0..50 {
        tx.send(vec![0.0; 320]).unwrap();
    }
    let dispatched = wait_until(Duration::from_secs(3), || {
        !sink.groups.lock().unwrap().is_empty()
    });
    let muted = wait_until(Duration::from_secs(1), || {
        status.lock().unwrap().input_paused
    });
    let resumed = wait_until(Duration::from_secs(5), || {
        !status.lock().unwrap().input_paused
    });

    drop(tx);
    handle.join().unwrap();

    assert!(dispatched, "the burst must produce a phrase");
    assert!(muted, "the input gate must close while the turn is live");
    assert!(
        resumed,
        "the gate must reopen after a no-PCM cancel is acked"
    );
    assert_eq!(status.lock().unwrap().cancel_timeouts, 0);
}

#[test]
fn unacknowledged_cancel_stalls_and_never_unmutes() {
    let specs = vec![spec(
        "hello ",
        FinalizeOutcome::Final(ScriptedText::new("hello ", "")),
    )];
    let sink = Arc::new(HoldSink::default());
    let (tx, output, _inflight, status, handle) = spawn_run(
        specs,
        Arc::clone(&sink) as Arc<dyn PhraseSink + Send + Sync>,
        Arc::new(RealClock),
        Status::default(),
        None,
    );
    let feed = Arc::new(AtomicBool::new(true));
    let feeder = paced_feeder(&tx, 0.0, Arc::clone(&feed));

    let failed = wait_until(Duration::from_secs(7), || {
        status.lock().unwrap().state == dex_core::State::Failed
    });

    feed.store(false, Ordering::SeqCst);
    drop(tx);
    let held = sink.held();
    sink.release();
    handle.join().unwrap();
    feeder.join().unwrap();

    assert!(
        failed,
        "an unacknowledged cancel must fail the synthesis domain"
    );
    assert!(output.is_empty(), "no late PCM may be queued");
    assert_eq!(held, 1);
    let status = status.lock().unwrap();
    assert_eq!(status.failure_domain, FailureDomain::Synthesis);
    assert_eq!(status.synthesis_state, SynthesisState::Stalled);
    assert_eq!(status.cancel_timeouts, 1);
    assert_eq!(status.cancel_requests, 1);
    assert!(status.input_paused, "a stalled cancel must never unmute");
}

#[test]
fn input_is_muted_while_output_is_queued() {
    let specs = vec![spec(
        "hello ",
        FinalizeOutcome::Final(ScriptedText::default()),
    )];
    let sink = Arc::new(RecordingSink::default());
    let (tx, rx) = sync_channel::<Vec<f32>>(16);
    let mut recognizer = ScriptedRecognizer::new(specs);
    let status = Arc::new(Mutex::new(Status::default()));
    let output = Arc::new(PcmQueue::new(4096));
    output.push(&vec![0.1; 4096]);
    let inflight = InFlight::new();
    let status_thread = Arc::clone(&status);
    let sink_thread = Arc::clone(&sink);
    let handle = thread::spawn(move || {
        run_loop(
            rx,
            &mut recognizer,
            sink_thread.as_ref(),
            &status_thread,
            &output,
            &AutoClock::new(),
            &inflight,
            EndpointConfig::default(),
            None,
        )
        .unwrap();
    });

    tx.send(vec![0.5; 320]).unwrap();
    let muted = wait_until(Duration::from_secs(1), || {
        status.lock().unwrap().input_paused
    });
    drop(tx);
    handle.join().unwrap();
    assert!(muted, "microphone must be muted");
    assert!(sink.groups.lock().unwrap().is_empty());
}
