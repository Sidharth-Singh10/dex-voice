//! Hermetic tests for the extracted recognition loop seam.

use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use dex_audio::PcmQueue;
use dex_core::{
    Dispatch, EndpointConfig, FinalizeOutcome, InFlight, PhraseSink, RealClock, ScriptedRecognizer,
    ScriptedSpec, ScriptedStep, ScriptedText, SendOutcome, Status, run_loop,
};

#[derive(Default)]
struct RecordingSink {
    groups: Mutex<Vec<(Vec<String>, Instant)>>,
}

impl PhraseSink for RecordingSink {
    fn try_send(&self, dispatch: Dispatch) -> SendOutcome {
        self.groups
            .lock()
            .unwrap()
            .push((dispatch.words, dispatch.at));
        SendOutcome::Sent
    }
}

fn run(spec: ScriptedSpec) -> Vec<(Vec<String>, Instant)> {
    let (tx, rx) = sync_channel::<Vec<f32>>(4);
    tx.send(vec![0.5; 320]).unwrap();
    drop(tx);
    let mut recognizer = ScriptedRecognizer::new(vec![spec]);
    let sink = Arc::new(RecordingSink::default());
    let status = Arc::new(Mutex::new(Status::default()));
    let output = PcmQueue::new(4096);
    let inflight = InFlight::new();
    let sink_thread = Arc::clone(&sink);
    let status_thread = Arc::clone(&status);
    thread::spawn(move || {
        run_loop(
            rx,
            &mut recognizer,
            sink_thread.as_ref(),
            &status_thread,
            &output,
            &RealClock,
            &inflight,
            EndpointConfig::default(),
            None,
        )
        .unwrap();
    })
    .join()
    .unwrap();
    sink.groups.lock().unwrap().clone()
}

#[test]
fn committed_phrase_is_flushed_when_input_ends() {
    let spec = ScriptedSpec {
        steps: vec![ScriptedStep {
            after_frames: 1,
            text: ScriptedText::new("hello ", ""),
        }],
        finalize: FinalizeOutcome::Final(ScriptedText::new("hello ", "")),
        feed_error_at: None,
    };
    let groups = run(spec);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].0, vec!["hello".to_owned()]);
}

#[test]
fn trailing_word_is_recovered_by_finalize() {
    let spec = ScriptedSpec {
        steps: vec![ScriptedStep {
            after_frames: 1,
            text: ScriptedText::new("hello", ""),
        }],
        finalize: FinalizeOutcome::Final(ScriptedText::new("hello", "")),
        feed_error_at: None,
    };
    let groups = run(spec);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].0, vec!["hello".to_owned()]);
}

#[test]
fn group_timestamp_is_monotonic() {
    let spec = ScriptedSpec {
        steps: vec![ScriptedStep {
            after_frames: 1,
            text: ScriptedText::new("hello ", ""),
        }],
        finalize: FinalizeOutcome::Final(ScriptedText::new("hello ", "")),
        feed_error_at: None,
    };
    let before = Instant::now();
    let groups = run(spec);
    assert!(groups[0].1 >= before);
}
