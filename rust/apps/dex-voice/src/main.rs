//! Dex Voice: Rune control panel over the Rust ASR→TTS pipeline.

mod config;
mod self_test;
mod ui;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use dex_audio::{
    Capture, DeviceChoice, PcmQueue, Playback, Resampler, device_choices, output_sample_rate,
    start_capture, start_playback,
};
use dex_core::{
    DeviceAction, DeviceState, Engine, EngineConfig, FailureDomain, State as EngineState, Status,
    Supervisor,
};
use rune::{AppSpec, TextEdit, Tx};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectMenu {
    Voice,
    Input,
    Output,
}

struct State {
    settings: config::Settings,
    draft: config::Settings,
    settings_open: bool,
    open_select: Option<SelectMenu>,
    asr_edit: TextEdit,
    tts_edit: TextEdit,
    input_choices: Vec<DeviceChoice>,
    output_choices: Vec<DeviceChoice>,
    device_error: Option<String>,
    engine: Option<Arc<Engine>>,
    queue: Option<Arc<PcmQueue>>,
    capture: Option<Capture>,
    playback: Option<Playback>,
    feeder: Option<JoinHandle<()>>,
    raw_tx: Option<SyncSender<Vec<f32>>>,
    status: Status,
    error: Option<String>,
    last_poll: Instant,
    output_rate: u32,
    closing: bool,
    supervisor: Supervisor,
    last_pops: u64,
    last_pop_change: Instant,
}

impl Default for State {
    fn default() -> Self {
        let settings = config::load();
        let mut state = Self {
            draft: settings.clone(),
            asr_edit: TextEdit::new(&settings.asr_model),
            tts_edit: TextEdit::new(&settings.tts_model),
            settings,
            settings_open: false,
            open_select: None,
            input_choices: Vec::new(),
            output_choices: Vec::new(),
            device_error: None,
            engine: None,
            queue: None,
            capture: None,
            playback: None,
            feeder: None,
            raw_tx: None,
            status: Status::default(),
            error: None,
            last_poll: Instant::now() - Duration::from_secs(1),
            output_rate: 48_000,
            closing: false,
            supervisor: Supervisor::new(),
            last_pops: 0,
            last_pop_change: Instant::now(),
        };
        // Device names are needed by the console's routing panel before Settings
        // is opened. This only enumerates; it never opens a stream or changes a
        // system default.
        state.refresh_devices();
        if std::env::var_os("DEX_AUTOSTART").is_some() {
            state.start();
        }
        state
    }
}
impl State {
    fn open_settings(&mut self) {
        if self.engine.is_some() {
            return;
        }
        self.asr_edit = TextEdit::new(&self.settings.asr_model);
        self.tts_edit = TextEdit::new(&self.settings.tts_model);
        self.draft = self.settings.clone();
        self.open_select = None;
        self.error = None;
        self.refresh_devices();
        self.settings_open = true;
    }

    fn close_settings(&mut self) {
        self.settings_open = false;
        self.open_select = None;
        self.error = None;
        self.device_error = None;
    }

    fn refresh_devices(&mut self) {
        let mut errors = Vec::new();
        match device_choices(true) {
            Ok(choices) => self.input_choices = choices,
            Err(error) => {
                self.input_choices.clear();
                errors.push(format!("inputs: {error:#}"));
            }
        }
        match device_choices(false) {
            Ok(choices) => self.output_choices = choices,
            Err(error) => {
                self.output_choices.clear();
                errors.push(format!("outputs: {error:#}"));
            }
        }
        self.device_error = (!errors.is_empty()).then(|| errors.join(" · "));
    }

    fn save_settings(&mut self) {
        let asr = self.asr_edit.text.trim();
        let tts = self.tts_edit.text.trim();
        if !PathBuf::from(asr).is_file() || !PathBuf::from(tts).join("config.json").is_file() {
            self.error = Some("Select an existing ASR GGUF and a Phonon checkpoint folder.".into());
            return;
        }
        let voices = config::available_voices(&PathBuf::from(tts));
        if !voices.contains(&self.draft.voice) {
            self.error = Some(format!(
                "Voice {:?} is not installed in the selected checkpoint.",
                self.draft.voice
            ));
            return;
        }
        if let Err(error) = self.validate_output() {
            self.error = Some(error.to_string());
            return;
        }
        self.draft.asr_model = asr.to_owned();
        self.draft.tts_model = tts.to_owned();
        match config::save(&self.draft) {
            Ok(()) => {
                self.settings = self.draft.clone();
                self.error = None;
                self.close_settings();
            }
            Err(error) => self.error = Some(format!("Could not save settings: {error}")),
        }
    }

    fn validate_output(&self) -> Result<()> {
        if self.draft.output_device.is_empty() {
            anyhow::bail!("Choose a virtual microphone output, not system-default speakers.");
        }
        if !self
            .output_choices
            .iter()
            .any(|device| device.id == self.draft.output_device)
        {
            anyhow::bail!("Selected output is disconnected. Choose an available virtual sink.");
        }
        Ok(())
    }

    fn draft_dirty(&self) -> bool {
        self.draft.voice != self.settings.voice
            || self.draft.input_device != self.settings.input_device
            || self.draft.output_device != self.settings.output_device
            || self.asr_edit.text.trim() != self.settings.asr_model
            || self.tts_edit.text.trim() != self.settings.tts_model
    }

    fn choose_voice(&mut self, label: &str) {
        let voices = config::available_voices(&PathBuf::from(self.tts_edit.text.trim()));
        if let Some(id) = choose_voice_from(&voices, label) {
            self.draft.voice = id;
        }
        self.open_select = None;
    }

    fn choose_device(&mut self, input: bool, label: &str) {
        let (current, choices) = if input {
            (&mut self.draft.input_device, &self.input_choices)
        } else {
            (&mut self.draft.output_device, &self.output_choices)
        };
        if input && label == "System default" {
            current.clear();
        } else if let Some(index) = device_labels(choices)
            .iter()
            .position(|candidate| candidate == label)
        {
            *current = choices[index].id.clone();
        }
        self.open_select = None;
    }

    fn start(&mut self) {
        if self.engine.is_some() {
            return;
        }
        self.error = None;
        if let Err(error) = self.try_start() {
            self.error = Some(format!("{error:#}"));
            self.cleanup();
        }
        let _ = config::save(&self.settings);
    }

    fn try_start(&mut self) -> Result<()> {
        let settings = self.settings.clone();
        if settings.output_device.is_empty() && std::env::var_os("DEX_AUTOSTART").is_none() {
            anyhow::bail!(
                "Select the virtual microphone output in Settings before starting. Do not use system-default speakers."
            );
        }
        if dex_core::generation_retired_pending() {
            anyhow::bail!("The previous engine is still shutting down. Start again in a moment.");
        }
        // Supervisory budgets are per engine generation.
        self.supervisor = Supervisor::new();
        self.last_pops = 0;
        self.last_pop_change = Instant::now();
        let rate = output_sample_rate(&settings.output_device)?;
        // Favor complete playback while latency tuning is deferred.
        let queue = Arc::new(PcmQueue::new(rate as usize * 30));
        let config = EngineConfig {
            asr_model: PathBuf::from(&settings.asr_model),
            tts_model: PathBuf::from(&settings.tts_model),
            voice: settings.voice.clone(),
            device: settings.device.clone(),
            group_words: settings.group_words,
            group_ms: settings.group_ms,
            trim_tts: settings.trim_tts,
            output_rate: rate,
            ..EngineConfig::default()
        };
        let engine = Arc::new(Engine::start(config, Arc::clone(&queue)).context("start engine")?);

        self.engine = Some(engine);
        self.queue = Some(queue);
        self.output_rate = rate;
        self.status = Status {
            state: EngineState::Loading,
            ..Status::default()
        };
        Ok(())
    }

    fn cancel(&mut self) {
        if let Some(engine) = &self.engine {
            engine.cancel_speech();
        }
    }

    fn stop(&mut self) {
        // The UI never joins workers. Close capture, ask the engine to finish
        // its tail and drain in a control thread, then release playback once a
        // terminal state arrives (see `poll`).
        if let Some(engine) = &self.engine {
            engine.request_stop();
        }
        self.capture.take();
        self.raw_tx.take();
        self.feeder.take();
        self.closing = true;
        if self.engine.is_some() && self.status.state != EngineState::Failed {
            self.status.state = EngineState::Stopping;
        }
        let _ = config::save(&self.settings);
    }

    fn start_devices(&mut self) -> Result<()> {
        let engine = self.engine.clone().context("engine not started")?;
        let queue = self.queue.clone().context("no output queue")?;
        let playback =
            start_playback(&self.settings.output_device, queue).context("open playback device")?;
        let (tx, rx) = sync_channel::<Vec<f32>>(200);
        let stop_tx = tx.clone();
        let capture_engine = Arc::clone(&engine);
        let capture = start_capture(&self.settings.input_device, move |block| {
            if tx.try_send(block).is_err() {
                capture_engine.note_capture_drop();
            }
        })
        .context("open capture device")?;
        let input_rate = capture.sample_rate;
        let feeder_engine = Arc::clone(&engine);
        let feeder = thread::Builder::new()
            .name("dex-capture".into())
            .spawn(move || {
                let mut resampler =
                    Resampler::new(input_rate, 16_000).expect("sample rates are non-zero");
                while let Ok(block) = rx.recv() {
                    let pcm = resampler.process(&block);
                    if !pcm.is_empty() {
                        feeder_engine.push_input(&pcm);
                    }
                }
            })
            .context("spawn capture feeder")?;
        self.playback = Some(playback);
        self.capture = Some(capture);
        self.raw_tx = Some(stop_tx);
        self.feeder = Some(feeder);
        Ok(())
    }

    fn stop_devices(&mut self) {
        self.capture.take();
        self.raw_tx.take();
        self.playback.take();
        // Dropping the handle detaches the feeder; it exits when the capture
        // channel closes. The UI must not block on a worker join.
        self.feeder.take();
    }

    fn cleanup(&mut self) {
        if let Some(engine) = &self.engine {
            engine.request_stop();
        }
        self.stop_devices();
        self.queue = None;
        self.closing = true;
    }
}

/// Display labels for devices, made unique when two devices share a name. The
/// index maps back to `choices`, so selecting a duplicate picks the right id.
pub(crate) fn device_labels(choices: &[DeviceChoice]) -> Vec<String> {
    choices
        .iter()
        .enumerate()
        .map(|(index, choice)| {
            let duplicates = choices[..index]
                .iter()
                .filter(|earlier| earlier.label == choice.label)
                .count();
            if duplicates == 0 {
                choice.label.clone()
            } else {
                format!("{} ({})", choice.label, duplicates + 1)
            }
        })
        .collect()
}

pub(crate) fn device_label(choices: &[DeviceChoice], id: &str) -> String {
    if id.is_empty() {
        return "System default".into();
    }
    if choices.is_empty() {
        return "Not detected".into();
    }
    choices
        .iter()
        .position(|choice| choice.id == id)
        .map(|index| device_labels(choices)[index].clone())
        .unwrap_or_else(|| "Unavailable device".into())
}

/// Display labels for installed voices, numbered when two IDs share a friendly
/// name. The index maps back to `voices`, so choosing is never ambiguous.
pub(crate) fn voice_labels(voices: &[String]) -> Vec<String> {
    voices
        .iter()
        .enumerate()
        .map(|(index, id)| {
            let base = config::voice_label(id);
            let duplicates = voices[..index]
                .iter()
                .filter(|earlier| config::voice_label(earlier) == base)
                .count();
            if duplicates == 0 {
                base.to_owned()
            } else {
                format!("{base} ({})", duplicates + 1)
            }
        })
        .collect()
}

fn choose_voice_from(voices: &[String], label: &str) -> Option<String> {
    voice_labels(voices)
        .iter()
        .position(|candidate| candidate == label)
        .map(|index| voices[index].clone())
}

/// Preserve a more severe device state when merging the app supervisor's view
/// with the engine's own device state.
fn merge_device_state(a: dex_core::DeviceState, b: dex_core::DeviceState) -> dex_core::DeviceState {
    use dex_core::DeviceState;
    if a == DeviceState::Lost || b == DeviceState::Lost {
        DeviceState::Lost
    } else if a == DeviceState::Recovering || b == DeviceState::Recovering {
        DeviceState::Recovering
    } else {
        DeviceState::Ready
    }
}
/// The engine only writes these when a TTS chunk is committed, so playback and
/// overflow need the live queue to avoid looking frozen.
fn refresh_queue(status: &mut Status, queue: Option<&PcmQueue>, rate: u32) {
    let seconds = |samples: u64| samples as f64 / f64::from(rate.max(1));
    match queue {
        Some(queue) => {
            status.queued_seconds = seconds(queue.len() as u64);
            status.dropped_seconds = seconds(queue.dropped());
        }
        None if status.state == EngineState::Idle => {
            status.queued_seconds = 0.0;
            status.dropped_seconds = 0.0;
        }
        None => {}
    }
}

#[cfg(test)]
impl State {
    /// A hermetic state for headless UI tests: no config read, no device
    /// enumeration, and no `DEX_AUTOSTART` side effects.
    pub(crate) fn test_fixture() -> State {
        let settings = config::Settings {
            asr_model: "/test/asr.gguf".into(),
            tts_model: "/test/phonon".into(),
            ..config::Settings::default()
        };
        State {
            draft: settings.clone(),
            asr_edit: TextEdit::new(&settings.asr_model),
            tts_edit: TextEdit::new(&settings.tts_model),
            settings,
            settings_open: false,
            open_select: None,
            input_choices: Vec::new(),
            output_choices: Vec::new(),
            device_error: None,
            engine: None,
            queue: None,
            capture: None,
            playback: None,
            feeder: None,
            raw_tx: None,
            status: Status::default(),
            error: None,
            last_poll: Instant::now() - Duration::from_secs(1),
            output_rate: 48_000,
            closing: false,
            supervisor: Supervisor::new(),
            last_pops: 0,
            last_pop_change: Instant::now(),
        }
    }
}

fn on_key(s: &mut State, event: &rune::KeyEvent, _tx: &mut rune::Tx) -> bool {
    if event.key == rune::Key::Escape && !s.settings_open {
        s.cancel();
        return true;
    }
    false
}

fn poll(s: &mut State, _clock: f64, tx: &mut Tx) -> bool {
    tx.wake_after(Duration::from_millis(200));
    if s.last_poll.elapsed() < Duration::from_millis(200) {
        return false;
    }
    s.last_poll = Instant::now();
    let mut changed = false;

    // Finish an asynchronous stop: the engine control thread owns the bounded
    // drain/join, so the UI only observes the terminal state and then releases
    // playback.
    if s.closing {
        if let Some(engine) = &s.engine {
            let mut status = engine.status();
            // A device failure that originated in the app is not visible in the
            // engine's status; keep it while the engine finishes stopping.
            if s.status.state == EngineState::Failed
                && s.status.failure_domain == FailureDomain::Device
            {
                status.state = EngineState::Failed;
                status.failure_domain = FailureDomain::Device;
                status.device_state = DeviceState::Lost;
                status.error = s.status.error.clone();
            }
            let terminal = matches!(
                status.state,
                EngineState::Idle | EngineState::Failed | EngineState::Recovering
            );
            s.status = status;
            if terminal {
                s.playback.take();
                s.queue = None;
                s.engine = None;
                s.closing = false;
            }
        } else {
            s.closing = false;
        }
        changed = true;
        return changed;
    }

    let Some(engine) = s.engine.clone() else {
        return changed;
    };
    let mut status = engine.status();
    refresh_queue(&mut status, s.queue.as_deref(), s.output_rate);
    status.device_state = merge_device_state(status.device_state, s.supervisor.device);
    let previous = s.status.clone();
    let mut changed = changed || status != s.status;

    // A playback that stops consuming queued audio for two seconds is a device
    // stall, not a synthesis failure. Reopen exactly once, then fail the device
    // domain.
    if status.state == EngineState::Running
        && s.capture.is_some()
        && let Some(queue) = s.queue.clone()
    {
        let pops = queue.pops();
        if pops != s.last_pops {
            s.last_pops = pops;
            s.last_pop_change = Instant::now();
        } else if !queue.is_empty() && s.last_pop_change.elapsed() > Duration::from_secs(2) {
            match s.supervisor.on_playback_stall() {
                DeviceAction::Reopen => {
                    s.playback.take();
                    let cleared = queue.clear();
                    match start_playback(&s.settings.output_device, Arc::clone(&queue)) {
                        Ok(playback) => {
                            s.playback = Some(playback);
                            s.supervisor.on_playback_recovered();
                            status.device_state =
                                merge_device_state(status.device_state, s.supervisor.device);
                            status.device_recoveries = s.supervisor.device_recoveries() as u64;
                            status.queue_cleared_samples += cleared as u64;
                            s.last_pop_change = Instant::now();
                        }
                        Err(error) => {
                            s.supervisor.fail_device();
                            status.device_state = DeviceState::Lost;
                            status.failure_domain = FailureDomain::Device;
                            status.state = EngineState::Failed;
                            status.error = Some(format!("Audio device recovery failed: {error:#}"));
                            s.error = status.error.clone();
                            engine.request_stop();
                            s.stop_devices();
                            s.closing = true;
                        }
                    }
                }
                DeviceAction::Fail => {
                    s.supervisor.fail_device();
                    status.device_state = DeviceState::Lost;
                    status.failure_domain = FailureDomain::Device;
                    status.device_recovery_failures += 1;
                    status.state = EngineState::Failed;
                    status.error = Some("Audio output stalled and could not be recovered.".into());
                    s.error = status.error.clone();
                    engine.request_stop();
                    s.stop_devices();
                    s.closing = true;
                }
            }
            changed = true;
        }
    }

    s.status = status;
    if s.status.transcript != previous.transcript || s.status.state != previous.state {
        eprintln!(
            "app: {:?} words={} p50={:?} transcript={}",
            s.status.state, s.status.words, s.status.p50_ms, s.status.transcript
        );
    }

    // Open the microphone and speakers only once both models are ready, so
    // loading does not discard seconds of captured audio.
    if s.status.state == EngineState::Running && s.capture.is_none() && s.error.is_none() {
        if let Err(error) = s.start_devices() {
            s.error = Some(format!("{error:#}"));
            s.status.state = EngineState::Failed;
            s.cleanup();
            changed = true;
        } else {
            changed = true;
        }
    }
    if s.status.state == EngineState::Failed && !s.closing {
        s.stop_devices();
    }
    changed
}

fn scene_ready_audio(s: &mut State) {
    if s.input_choices.is_empty() {
        s.input_choices.push(DeviceChoice {
            id: "fixture:jade".into(),
            label: "JadeAudio JA11 Mono".into(),
        });
    }
    if s.output_choices.is_empty() {
        s.output_choices.push(DeviceChoice {
            id: "fixture:dexvoice".into(),
            label: "DexVoice".into(),
        });
    }
    if s.settings.output_device.is_empty() {
        s.settings.output_device = s
            .output_choices
            .iter()
            .find(|choice| choice.label.to_lowercase().contains("dexvoice"))
            .or_else(|| s.output_choices.first())
            .map(|choice| choice.id.clone())
            .unwrap_or_default();
    }
}

fn scene_settings(s: &mut State) {
    s.open_settings();
    if s.input_choices.is_empty() {
        s.input_choices.push(DeviceChoice {
            id: "fixture:jade".into(),
            label: "JadeAudio JA11 Mono".into(),
        });
    }
    if s.output_choices.is_empty() {
        s.output_choices.push(DeviceChoice {
            id: "fixture:dexvoice".into(),
            label: "DexVoice".into(),
        });
    }
    if s.draft.input_device.is_empty() {
        s.draft.input_device = s.input_choices[0].id.clone();
    }
    if s.draft.output_device.is_empty() {
        s.draft.output_device = s
            .output_choices
            .iter()
            .find(|choice| choice.label.to_lowercase().contains("dexvoice"))
            .or_else(|| s.output_choices.first())
            .map(|choice| choice.id.clone())
            .unwrap_or_default();
    }
}

fn main() {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--self-test")) {
        let out = std::env::args_os()
            .nth(2)
            .map(PathBuf::from)
            .unwrap_or_else(|| config::data_dir().join("self-test.json"));
        if let Err(error) = self_test::run(&out) {
            eprintln!("self-test failed: {error:#}");
            std::process::exit(1);
        }
        return;
    }
    rune::run(AppSpec::<State> {
        min_size: Some((760.0, 600.0)),
        scenes: &[
            ("idle", |s| {
                s.status.state = EngineState::Idle;
            }),
            ("loading", |s| {
                scene_ready_audio(s);
                s.status.state = EngineState::Loading;
            }),
            ("listening", |s| {
                scene_ready_audio(s);
                s.status.state = EngineState::Running;
                s.status.transcript = "Well, I don't wish to see it any more.".into();
                s.status.words = 9;
                s.status.p50_ms = Some(281.0);
                s.status.p95_ms = Some(684.0);
            }),
            ("running", |s| {
                scene_ready_audio(s);
                s.status.state = EngineState::Running;
                s.status.transcript = "Well, I don't wish to see it any more.".into();
                s.status.words = 9;
                s.status.p50_ms = Some(281.0);
                s.status.p95_ms = Some(684.0);
            }),
            ("speaking", |s| {
                scene_ready_audio(s);
                s.status.state = EngineState::Running;
                s.status.input_paused = true;
                s.status.transcript = "Well, I don't wish to see it any more.".into();
                s.status.words = 9;
                s.status.queued_seconds = 1.4;
                s.status.p50_ms = Some(281.0);
                s.status.p95_ms = Some(684.0);
            }),
            ("stopping", |s| {
                scene_ready_audio(s);
                s.status.state = EngineState::Stopping;
            }),
            ("failed", |s| {
                scene_ready_audio(s);
                s.status.state = EngineState::Failed;
                s.status.error = Some("no GPU matching \"nvidia\"".into());
            }),
            ("settings", scene_settings),
        ],
        poll: Some(poll),
        on_key: Some(on_key),
        ..AppSpec::new("Dex Voice", (900.0, 640.0), State::default, ui::view)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use dex_audio::PcmQueue;

    #[test]
    fn start_requires_virtual_output_instead_of_speakers() {
        let mut state = State::test_fixture();
        state.settings.output_device.clear();
        let error = state.try_start().expect_err("blank output must be refused");
        assert!(
            format!("{error:#}").contains("virtual microphone"),
            "{error:#}"
        );
        assert!(state.engine.is_none());
    }

    #[test]
    fn settings_reject_missing_or_disconnected_output() {
        let mut state = State::test_fixture();
        state.draft.output_device.clear();
        assert!(state.validate_output().is_err());
        state.draft.output_device = "pulseaudio:missing-virtual-sink".into();
        state.output_choices.clear();
        assert!(state.validate_output().is_err());
    }

    #[test]
    fn duplicate_device_names_select_the_right_device_id() {
        let choices = vec![
            DeviceChoice {
                id: "first".into(),
                label: "Studio Mic".into(),
            },
            DeviceChoice {
                id: "second".into(),
                label: "Studio Mic".into(),
            },
        ];
        assert_eq!(
            device_labels(&choices),
            vec!["Studio Mic", "Studio Mic (2)"]
        );
        assert_eq!(device_label(&choices, "second"), "Studio Mic (2)");
        let mut state = State::test_fixture();
        state.input_choices = choices;
        state.choose_device(true, "Studio Mic (2)");
        assert_eq!(state.draft.input_device, "second");
    }

    #[test]
    fn voice_labels_disambiguate_duplicate_friendly_names() {
        let voices = vec!["NbpkqMVS3CJeq2j8".to_owned(), "Zoey".to_owned()];
        assert_eq!(voice_labels(&voices), vec!["Zoey", "Zoey (2)"]);
        assert_eq!(
            choose_voice_from(&voices, "Zoey (2)").as_deref(),
            Some("Zoey")
        );
        assert_eq!(
            choose_voice_from(&voices, "Zoey").as_deref(),
            Some("NbpkqMVS3CJeq2j8")
        );
    }

    #[test]
    fn queue_timing_comes_from_live_samples_and_rate() {
        let queue = PcmQueue::new(10);
        queue.push(&[0.0; 20]);
        let mut status = Status {
            state: EngineState::Running,
            ..Status::default()
        };
        refresh_queue(&mut status, Some(&queue), 100);
        assert!((status.queued_seconds - 0.1).abs() < f64::EPSILON);
        assert!((status.dropped_seconds - 0.1).abs() < f64::EPSILON);

        status.state = EngineState::Idle;
        refresh_queue(&mut status, None, 100);
        assert_eq!(status.queued_seconds, 0.0);
        assert_eq!(status.dropped_seconds, 0.0);
    }

    #[test]
    fn draft_dirty_tracks_device_voice_and_paths() {
        let mut state = State::test_fixture();
        assert!(!state.draft_dirty());
        state.draft.output_device.push_str("changed");
        assert!(state.draft_dirty());
        state.draft.output_device = state.settings.output_device.clone();
        state.asr_edit = TextEdit::new("/tmp/other.gguf");
        assert!(state.draft_dirty());
    }
}
