//! Hermetic integration tests for P0 speech continuity.
//!
//! These exercise the production `run_loop`/`Engine` entry points with scripted
//! recognizers and deterministic clocks; no model, GPU or microphone is involved.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::sync_channel;
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