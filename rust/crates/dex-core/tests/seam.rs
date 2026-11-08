//! Hermetic tests for the extracted recognition loop seam.

use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use dex_audio::PcmQueue;
use dex_core::{
    FinalizeOutcome, Group, PhraseSink, RealClock, ScriptedRecognizer, ScriptedSpec, ScriptedStep,
    ScriptedText, Status, run_loop,
};

#[derive(Default)]
struct RecordingSink {
    groups: Mutex<Vec<Group>>,
}

impl PhraseSink for RecordingSink {
    fn send(&self, group: Group) -> bool {
        self.groups.lock().unwrap().push(group);
        true
    }
}

fn run(spec: ScriptedSpec) -> Vec<Vec<String>> {
    let (tx, rx) = sync_channel::<Vec<f32>>(4);
    tx.send(vec![0.5; 320]).unwrap();
    drop(tx);
    let mut recognizer = ScriptedRecognizer::new(vec![spec]);
    let sink = Arc::new(RecordingSink::default());
    let status = Arc::new(Mutex::new(Status::default()));
    let output = PcmQueue::new(4096);
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
        )
        .unwrap();
    })
    .join()
    .unwrap();
    sink.groups
        .lock()
        .unwrap()
        .iter()
        .map(|group| group.words.clone())
        .collect()
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
    assert_eq!(run(spec), vec![vec!["hello".to_owned()]]);
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
    assert_eq!(run(spec), vec![vec!["hello".to_owned()]]);
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
    let (tx, rx) = sync_channel::<Vec<f32>>(4);
    tx.send(vec![0.5; 320]).unwrap();
    drop(tx);
    let mut recognizer = ScriptedRecognizer::new(vec![spec]);
    let sink = Arc::new(RecordingSink::default());
    let status = Arc::new(Mutex::new(Status::default()));
    let output = PcmQueue::new(4096);
    let before = Instant::now();
    run_loop(
        rx,
        &mut recognizer,
        sink.as_ref(),
        &status,
        &output,
        &RealClock,
    )
    .unwrap();
    let groups = sink.groups.lock().unwrap();
    assert!(groups[0].at >= before);
}
