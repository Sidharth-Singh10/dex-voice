//! Hermetic integration tests for P0 speech continuity.
//!
//! These exercise the production `run_loop`/`Engine` entry points with scripted
//! recognizers and deterministic clocks; no model, GPU or microphone is involved.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use dex_audio::PcmQueue;
use dex_core::{
    Clock, EndpointConfig, FinalizeOutcome, InFlight, PhraseSink, ScriptedRecognizer, ScriptedSpec,
    ScriptedStep, ScriptedText, SendOutcome, Status, run_loop,
};

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
    fn try_send(&self, dispatch: dex_core::Dispatch) -> SendOutcome {
        self.groups.lock().unwrap().push(dispatch.words);
        SendOutcome::Sent
    }
}

struct DisconnectedSink;

impl PhraseSink for DisconnectedSink {
    fn try_send(&self, _dispatch: dex_core::Dispatch) -> SendOutcome {
        SendOutcome::Disconnected
    }
}

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

#[test]
fn synthesis_failure_is_reported_and_stops_recognition() {
    let specs = vec![spec("hello ", FinalizeOutcome::Error)];
    let (tx, rx) = sync_channel::<Vec<f32>>(4096);
    let mut recognizer = ScriptedRecognizer::new(specs);
    let status = Arc::new(Mutex::new(Status::default()));
    let output = Arc::new(PcmQueue::new(4096));
    let inflight = InFlight::new();
    let status_thread = Arc::clone(&status);
    let sink = Arc::new(DisconnectedSink);
    let sink: Arc<dyn PhraseSink + Send + Sync> = sink;
    let handle = thread::spawn(move || {
        run_loop(
            rx,
            &mut recognizer,
            sink.as_ref(),
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
    for _ in 0..60 {
        let _ = tx.send(vec![0.0; 320]);
    }
    drop(tx);
    handle.join().unwrap();

    let status = status.lock().unwrap();
    assert_eq!(status.state, dex_core::State::Failed);
    assert_eq!(status.failure_domain, dex_core::FailureDomain::Synthesis);
    assert!(status.synthesis_stalls >= 1);
}

/// Sink that fails once then succeeds, exercising the one-shot TTS restart
/// path. Events are sent on a channel so the test can synchronise on a
/// barrier instead of sleeping on wall-clock time.
#[derive(Debug, PartialEq, Eq)]
enum ProbeEvent {
    Disconnected,
    Restarted,
    Sent(Vec<String>),
}

struct RestartOnceSink {
    calls: AtomicUsize,
    groups: Mutex<Vec<Vec<String>>>,
    events: Mutex<std::sync::mpsc::Sender<ProbeEvent>>,
}

impl RestartOnceSink {
    fn new() -> (Self, std::sync::mpsc::Receiver<ProbeEvent>) {
        let (tx, rx) = channel();
        (
            Self {
                calls: AtomicUsize::new(0),
                groups: Mutex::new(Vec::new()),
                events: Mutex::new(tx),
            },
            rx,
        )
    }
}

impl PhraseSink for RestartOnceSink {
    fn try_send(&self, dispatch: dex_core::Dispatch) -> SendOutcome {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        if n == 0 {
            let _ = self.events.lock().unwrap().send(ProbeEvent::Disconnected);
            SendOutcome::Disconnected
        } else {
            let words = dispatch.words.clone();
            self.groups.lock().unwrap().push(words.clone());
            let _ = self.events.lock().unwrap().send(ProbeEvent::Sent(words));
            SendOutcome::Sent
        }
    }
    fn restart(&self) -> bool {
        let _ = self.events.lock().unwrap().send(ProbeEvent::Restarted);
        true
    }
}

#[test]
fn restart_succeeds_and_subsequent_phrase_speaks() {
    // Deterministic, event-barrier driven: Disconnected -> restart -> the
    // retried phrase succeeds, then the next phrase (again) succeeds.
    // No sleep / wall-clock wait; AutoClock advances per loop iteration.
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
    let (tx, rx) = sync_channel::<Vec<f32>>(4096);
    let mut recognizer = ScriptedRecognizer::new(specs);
    let status = Arc::new(Mutex::new(Status::default()));
    let output = Arc::new(PcmQueue::new(4096));
    let inflight = InFlight::new();
    let status_thread = Arc::clone(&status);
    let (sink, events) = RestartOnceSink::new();
    let sink = Arc::new(sink);
    let sink_dyn: Arc<dyn PhraseSink + Send + Sync> = sink.clone();
    let handle = thread::spawn(move || {
        run_loop(
            rx,
            &mut recognizer,
            sink_dyn.as_ref(),
            &status_thread,
            &output,
            &AutoClock::new(),
            &inflight,
            EndpointConfig::default(),
            None,
        )
        .unwrap();
    });

    // Phrase 1: speech + 800 ms of silence endpoints at the audio-frame level.
    tx.send(vec![0.5; 320]).unwrap();
    for _ in 0..60 {
        tx.send(vec![0.0; 320]).unwrap();
    }
    // Barrier: Disconnected -> Restarted -> retried Sent(hello), in order.
    assert_eq!(events.recv().unwrap(), ProbeEvent::Disconnected);
    assert_eq!(events.recv().unwrap(), ProbeEvent::Restarted);
    assert_eq!(
        events.recv().unwrap(),
        ProbeEvent::Sent(vec!["hello".to_owned()])
    );
    assert_eq!(
        sink.groups.lock().unwrap().clone(),
        vec![vec!["hello".to_owned()]]
    );

    // Phrase 2: 600 silent frames drive the empty middle stream to its
    // unarmed cap (500) and then the "again" stream to its silence endpoint.
    for _ in 0..600 {
        tx.send(vec![0.0; 320]).unwrap();
    }
    assert_eq!(
        events.recv().unwrap(),
        ProbeEvent::Sent(vec!["again".to_owned()])
    );

    drop(tx);
    handle.join().unwrap();

    assert_eq!(
        sink.groups.lock().unwrap().clone(),
        vec![vec!["hello".to_owned()], vec!["again".to_owned()]]
    );
    assert_eq!(sink.calls.load(Ordering::SeqCst), 3);
    assert!(events.try_recv().is_err(), "no further sink events");
    let status = status.lock().unwrap();
    assert_eq!(status.tts_restarts, 1, "exactly one restart");
    assert_eq!(status.dispatch_failures, 1);
    assert_eq!(status.tts_restart_failures, 0);
    assert_eq!(status.synthesis_stalls, 0);
    assert_ne!(status.state, dex_core::State::Failed);
    assert_eq!(status.transcript, "hello again ");
}

#[test]
fn speech_error_marker_is_recorded_on_disconnect_failure() {
    let specs = vec![spec("hello ", FinalizeOutcome::Error)];
    let (tx, rx) = sync_channel::<Vec<f32>>(4096);
    let mut recognizer = ScriptedRecognizer::new(specs);
    let status = Arc::new(Mutex::new(Status::default()));
    let output = Arc::new(PcmQueue::new(4096));
    let inflight = InFlight::new();
    let status_thread = Arc::clone(&status);
    let sink = Arc::new(DisconnectedSink);
    let sink_dyn: Arc<dyn PhraseSink + Send + Sync> = sink;
    let handle = thread::spawn(move || {
        run_loop(
            rx,
            &mut recognizer,
            sink_dyn.as_ref(),
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
    for _ in 0..60 {
        let _ = tx.send(vec![0.0; 320]);
    }
    drop(tx);
    handle.join().unwrap();

    let status = status.lock().unwrap();
    assert!(
        status
            .transcript
            .contains("[not spoken: synthesis unavailable]")
    );
    assert!(status.synthesis_stalls >= 1);
}

#[test]
fn failed_engine_does_not_spin_or_emit() {
    let specs = vec![ScriptedSpec {
        steps: Vec::new(),
        finalize: FinalizeOutcome::Final(ScriptedText::new("hello ", "")),
        feed_error_at: None,
    }];
    let (tx, rx) = sync_channel::<Vec<f32>>(4096);
    let mut recognizer = ScriptedRecognizer::new(specs);
    let status = Arc::new(Mutex::new(Status {
        state: dex_core::State::Failed,
        ..Status::default()
    }));
    let output = Arc::new(PcmQueue::new(4096));
    let inflight = InFlight::new();
    let status_thread = Arc::clone(&status);
    let recording = Arc::new(RecordingSink::default());
    let sink: Arc<dyn PhraseSink + Send + Sync> = recording.clone();
    let handle = thread::spawn(move || {
        run_loop(
            rx,
            &mut recognizer,
            sink.as_ref(),
            &status_thread,
            &output,
            &AutoClock::new(),
            &inflight,
            EndpointConfig::default(),
            None,
        )
        .unwrap();
    });

    for _ in 0..100 {
        let _ = tx.send(vec![0.0; 320]);
    }
    drop(tx);
    handle.join().unwrap();

    assert!(recording.groups.lock().unwrap().is_empty());
    assert_eq!(status.lock().unwrap().state, dex_core::State::Failed);
}
