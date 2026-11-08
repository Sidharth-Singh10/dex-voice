//! Hermetic behavioural tests for P0 speech continuity.
//!
//! They drive the production `run_loop` with a scripted recognizer, a
//! deterministic clock and a recording sink: no model, GPU or microphone is
//! involved.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use dex_audio::PcmQueue;
use dex_core::{
    Clock, Dispatch, EndpointConfig, FinalizeOutcome, InFlight, PhraseSink, ScriptedRecognizer,
    ScriptedSpec, ScriptedStep, ScriptedText, SendOutcome, Status, run_loop,
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
    groups: Mutex<Vec<(Vec<String>, Instant)>>,
    notify: Mutex<Option<SyncSender<()>>>,
}

impl PhraseSink for RecordingSink {
    fn try_send(&self, dispatch: Dispatch) -> SendOutcome {
        self.groups
            .lock()
            .unwrap()
            .push((dispatch.words, dispatch.at));
        if let Some(notify) = self.notify.lock().unwrap().as_ref() {
            let _ = notify.try_send(());
        }
        SendOutcome::Sent
    }
}

fn sink() -> (Arc<RecordingSink>, std::sync::mpsc::Receiver<()>) {
    let (tx, rx) = sync_channel(16);
    let sink = Arc::new(RecordingSink::default());
    *sink.notify.lock().unwrap() = Some(tx);
    (sink, rx)
}

fn spawn(
    spec: ScriptedSpec,
    sink: Arc<RecordingSink>,
) -> (
    SyncSender<Vec<f32>>,
    Arc<Mutex<Status>>,
    thread::JoinHandle<()>,
) {
    let (tx, rx) = sync_channel::<Vec<f32>>(4096);
    let mut recognizer = ScriptedRecognizer::new(vec![spec]);
    let status = Arc::new(Mutex::new(Status::default()));
    let output = PcmQueue::new(4096);
    let inflight = InFlight::new();
    let status_thread = Arc::clone(&status);
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
    (tx, status, handle)
}

fn words(groups: &[(Vec<String>, Instant)]) -> Vec<String> {
    groups
        .iter()
        .map(|(words, _)| words.join(" "))
        .collect::<Vec<_>>()
}

#[test]
fn one_word_phrase_flushes_after_800ms_of_silence() {
    let spec = ScriptedSpec {
        steps: vec![ScriptedStep {
            after_frames: 1,
            text: ScriptedText::new("hello", ""),
        }],
        finalize: FinalizeOutcome::Final(ScriptedText::new("hello", "")),
        feed_error_at: None,
    };
    let (sink, notify) = sink();
    let (tx, _status, handle) = spawn(spec, Arc::clone(&sink));

    tx.send(vec![0.5; 320]).unwrap();
    for _ in 0..100 {
        tx.send(vec![0.0; 320]).unwrap();
    }
    let flushed = notify.recv_timeout(Duration::from_millis(500)).is_ok();
    drop(tx);
    handle.join().unwrap();

    assert!(
        flushed,
        "a one-word phrase must be dispatched after 800 ms of audio silence, before input ends"
    );
}

#[test]
fn tentative_display_arms_the_uninterrupted_speech_cap() {
    let spec = ScriptedSpec {
        steps: vec![ScriptedStep {
            after_frames: 1,
            text: ScriptedText::new("", "hello"),
        }],
        finalize: FinalizeOutcome::Final(ScriptedText::new("", "hello")),
        feed_error_at: None,
    };
    let (sink, notify) = sink();
    let (tx, _status, handle) = spawn(spec, Arc::clone(&sink));

    for _ in 0..700 {
        tx.send(vec![0.5; 320]).unwrap();
    }
    let flushed = notify.recv_timeout(Duration::from_millis(800)).is_ok();
    drop(tx);
    handle.join().unwrap();

    assert!(
        flushed,
        "a phrase visible only in display() must arm and hit the 10 s cap"
    );
}

#[test]
fn not_final_finalize_emits_only_pre_finalize_committed_words() {
    let spec = ScriptedSpec {
        steps: vec![ScriptedStep {
            after_frames: 1,
            text: ScriptedText::new("hello ", ""),
        }],
        finalize: FinalizeOutcome::NotFinal(ScriptedText::new("hello ", "world")),
        feed_error_at: None,
    };
    let (sink, notify) = sink();
    let (tx, _status, handle) = spawn(spec, Arc::clone(&sink));

    tx.send(vec![0.5; 320]).unwrap();
    for _ in 0..200 {
        tx.send(vec![0.0; 320]).unwrap();
    }
    let _ = notify.recv_timeout(Duration::from_millis(500));
    drop(tx);
    handle.join().unwrap();

    let groups = sink.groups.lock().unwrap();
    assert_eq!(
        words(&groups),
        vec!["hello".to_owned()],
        "a non-final finalize must not smuggle the tentative word into speech"
    );
}

#[test]
fn error_finalize_is_non_fatal_and_emits_committed_words() {
    let spec = ScriptedSpec {
        steps: vec![ScriptedStep {
            after_frames: 1,
            text: ScriptedText::new("hello ", ""),
        }],
        finalize: FinalizeOutcome::Error,
        feed_error_at: None,
    };
    let (sink, notify) = sink();
    let (tx, status, handle) = spawn(spec, Arc::clone(&sink));

    tx.send(vec![0.5; 320]).unwrap();
    for _ in 0..60 {
        tx.send(vec![0.0; 320]).unwrap();
    }
    let flushed = notify.recv_timeout(Duration::from_millis(500)).is_ok();
    drop(tx);
    handle.join().unwrap();

    assert!(flushed);
    assert_eq!(
        words(&sink.groups.lock().unwrap()),
        vec!["hello".to_owned()]
    );
    assert_eq!(status.lock().unwrap().finalize_errors, 1);
}

#[test]
fn an_empty_finalize_emits_no_phrase() {
    let spec = ScriptedSpec {
        steps: Vec::new(),
        finalize: FinalizeOutcome::Final(ScriptedText::default()),
        feed_error_at: None,
    };
    let (sink, notify) = sink();
    let (tx, status, handle) = spawn(spec, Arc::clone(&sink));

    for _ in 0..600 {
        tx.send(vec![0.0; 320]).unwrap();
    }
    let _ = notify.recv_timeout(Duration::from_millis(300));
    drop(tx);
    handle.join().unwrap();

    assert!(sink.groups.lock().unwrap().is_empty());
    assert_eq!(status.lock().unwrap().empty_finalize, 1);
    assert_eq!(status.lock().unwrap().idle_cap_resets, 1);
}

#[test]
fn unarmed_phrase_resets_without_finalize_or_emit() {
    let spec = ScriptedSpec {
        steps: Vec::new(),
        finalize: FinalizeOutcome::Final(ScriptedText::default()),
        feed_error_at: None,
    };
    let (sink, notify) = sink();
    let (tx, status, handle) = spawn(spec, Arc::clone(&sink));

    for _ in 0..600 {
        tx.send(vec![0.0; 320]).unwrap();
    }
    let _ = notify.recv_timeout(Duration::from_millis(300));
    drop(tx);
    handle.join().unwrap();

    assert!(sink.groups.lock().unwrap().is_empty());
    assert_eq!(status.lock().unwrap().idle_cap_resets, 1);
}

#[test]
fn input_is_muted_while_output_is_queued() {
    let spec = ScriptedSpec {
        steps: vec![ScriptedStep {
            after_frames: 1,
            text: ScriptedText::new("hello ", ""),
        }],
        finalize: FinalizeOutcome::Final(ScriptedText::default()),
        feed_error_at: None,
    };
    let (sink, _notify) = sink();
    let (tx, rx) = sync_channel::<Vec<f32>>(16);
    let mut recognizer = ScriptedRecognizer::new(vec![spec]);
    let status = Arc::new(Mutex::new(Status::default()));
    let output = PcmQueue::new(4096);
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
    let deadline = Instant::now() + Duration::from_secs(1);
    while !status.lock().unwrap().input_paused && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        status.lock().unwrap().input_paused,
        "microphone must be muted"
    );
    drop(tx);
    handle.join().unwrap();
    assert!(sink.groups.lock().unwrap().is_empty());
}
