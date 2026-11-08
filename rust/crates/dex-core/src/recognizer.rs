//! Recognizer seam used by the realtime loop.
//!
//! Production streams wrap `transcribe.cpp`; tests drive the same loop with a
//! scripted implementation so no model or GPU is required.

use anyhow::Result;
use transcribe_cpp::{
    Backend, DeviceType, Model, ModelOptions, ParakeetBufferedStreamOptions, RunOptions,
    StreamExtension, StreamOptions, devices, init_backends_default,
};

use crate::EngineConfig;

/// Change metadata reported by a feed or finalize call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FeedUpdate {
    pub committed_changed: bool,
    pub tentative_changed: bool,
    pub is_final: bool,
}

/// Text snapshot from a recognizer stream.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextSnapshot {
    pub display: String,
    pub committed: String,
    pub tentative: String,
    pub full: String,
}

impl TextSnapshot {
    fn from_stream(text: transcribe_cpp::StreamText) -> Self {
        let display = text.display();
        Self {
            display,
            committed: text.committed,
            tentative: text.tentative,
            full: text.full,
        }
    }
}

/// One live recognizer stream.
pub trait RecogStream {
    fn feed(&mut self, pcm: &[f32]) -> Result<FeedUpdate>;
    fn finalize(&mut self) -> Result<FeedUpdate>;
    fn text(&self) -> TextSnapshot;
}

/// Opens a fresh recognizer stream for each phrase.
pub trait Recognizer {
    fn open(&mut self) -> Result<Box<dyn RecogStream + '_>>;
}

/// Real Parakeet stream adapter.
struct TranscribeStream<'a> {
    stream: transcribe_cpp::Stream<'a>,
}

impl RecogStream for TranscribeStream<'_> {
    fn feed(&mut self, pcm: &[f32]) -> Result<FeedUpdate> {
        let update = self.stream.feed(pcm)?;
        Ok(FeedUpdate {
            committed_changed: update.committed_changed,
            tentative_changed: update.tentative_changed,
            is_final: update.is_final,
        })
    }

    fn finalize(&mut self) -> Result<FeedUpdate> {
        let update = self.stream.finalize()?;
        Ok(FeedUpdate {
            committed_changed: update.committed_changed,
            tentative_changed: update.tentative_changed,
            is_final: update.is_final,
        })
    }

    fn text(&self) -> TextSnapshot {
        TextSnapshot::from_stream(self.stream.text())
    }
}

/// Owns the model session and creates phrase streams on demand.
pub struct TranscribeRecognizer {
    session: transcribe_cpp::Session,
    run: RunOptions,
    options: StreamOptions,
}

impl TranscribeRecognizer {
    /// Loads the model on the configured GPU and runs the warm-up stream.
    pub fn load(config: &EngineConfig) -> Result<Self> {
        init_backends_default().map_err(|error| anyhow::anyhow!("initialise backends: {error}"))?;
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
                anyhow::anyhow!(
                    "no GPU matching {:?}; saw {}",
                    config.device,
                    available
                        .iter()
                        .map(|device| device.description.clone())
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
        .map_err(|error| anyhow::anyhow!("load {}: {error}", config.asr_model.display()))?;
        let mut session = model
            .session()
            .map_err(|error| anyhow::anyhow!("create session: {error}"))?;
        let run = RunOptions {
            timestamps: transcribe_cpp::TimestampKind::Token,
            ..RunOptions::default()
        };
        let options = StreamOptions {
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
                .stream(&run, &options)
                .map_err(|error| anyhow::anyhow!("warmup stream: {error}"))?;
            warm.feed(&vec![0.0; 8000])
                .map_err(|error| anyhow::anyhow!("warmup feed: {error}"))?;
            warm.finalize()
                .map_err(|error| anyhow::anyhow!("warmup finalize: {error}"))?;
        }
        Ok(Self {
            session,
            run,
            options,
        })
    }

    fn open_stream(&mut self) -> Result<Box<dyn RecogStream + '_>> {
        let stream = self
            .session
            .stream(&self.run, &self.options)
            .map_err(|error| anyhow::anyhow!("start stream: {error}"))?;
        Ok(Box::new(TranscribeStream { stream }))
    }
}

impl Recognizer for TranscribeRecognizer {
    fn open(&mut self) -> Result<Box<dyn RecogStream + '_>> {
        self.open_stream()
    }
}

/// Text installed by a scripted step.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptedText {
    pub committed: String,
    pub tentative: String,
    pub full: String,
}

impl ScriptedText {
    pub fn new(committed: &str, tentative: &str) -> Self {
        Self {
            committed: committed.to_owned(),
            tentative: tentative.to_owned(),
            full: format!("{committed}{tentative}"),
        }
    }

    fn snapshot(&self) -> TextSnapshot {
        TextSnapshot {
            display: format!("{}{}", self.committed, self.tentative),
            committed: self.committed.clone(),
            tentative: self.tentative.clone(),
            full: if self.full.is_empty() {
                format!("{}{}", self.committed, self.tentative)
            } else {
                self.full.clone()
            },
        }
    }
}

/// A scripted update that becomes visible after `after_frames` fed frames.
#[derive(Clone, Debug)]
pub struct ScriptedStep {
    pub after_frames: u64,
    pub text: ScriptedText,
}

/// Finalize behavior for a scripted stream.
#[derive(Clone, Debug)]
pub enum FinalizeOutcome {
    Final(ScriptedText),
    NotFinal(ScriptedText),
    Error,
}

impl Default for FinalizeOutcome {
    fn default() -> Self {
        Self::Final(ScriptedText::default())
    }
}

/// Description of one scripted stream.
#[derive(Clone, Debug, Default)]
pub struct ScriptedSpec {
    pub steps: Vec<ScriptedStep>,
    pub finalize: FinalizeOutcome,
    pub feed_error_at: Option<u64>,
}

/// Recognizer used by hermetic tests. Each call to [`Recognizer::open`] consumes
/// the next spec; exhausted specs behave as an empty stream.
pub struct ScriptedRecognizer {
    specs: Vec<ScriptedSpec>,
    opened: usize,
}

impl ScriptedRecognizer {
    pub fn new(specs: Vec<ScriptedSpec>) -> Self {
        Self { specs, opened: 0 }
    }
}

impl Recognizer for ScriptedRecognizer {
    fn open(&mut self) -> Result<Box<dyn RecogStream + '_>> {
        let spec = self.specs.get(self.opened).cloned().unwrap_or_default();
        self.opened += 1;
        Ok(Box::new(ScriptedStream {
            spec,
            frames: 0,
            text: ScriptedText::default(),
            finalized: false,
        }))
    }
}

struct ScriptedStream {
    spec: ScriptedSpec,
    frames: u64,
    text: ScriptedText,
    finalized: bool,
}

impl RecogStream for ScriptedStream {
    fn feed(&mut self, _pcm: &[f32]) -> Result<FeedUpdate> {
        self.frames += 1;
        if self.spec.feed_error_at.is_some_and(|at| self.frames >= at) {
            anyhow::bail!("scripted feed failure");
        }
        let mut changed = false;
        for step in &self.spec.steps {
            if step.after_frames <= self.frames && step.text != self.text {
                self.text = step.text.clone();
                changed = true;
            }
        }
        Ok(FeedUpdate {
            committed_changed: changed,
            tentative_changed: changed,
            is_final: false,
        })
    }

    fn finalize(&mut self) -> Result<FeedUpdate> {
        if self.finalized {
            anyhow::bail!("scripted stream finalized twice");
        }
        self.finalized = true;
        let (text, is_final) = match &self.spec.finalize {
            FinalizeOutcome::Final(text) => (text.clone(), true),
            FinalizeOutcome::NotFinal(text) => (text.clone(), false),
            FinalizeOutcome::Error => anyhow::bail!("scripted finalize failure"),
        };
        let changed = text != self.text;
        self.text = text;
        Ok(FeedUpdate {
            committed_changed: changed,
            tentative_changed: changed,
            is_final,
        })
    }

    fn text(&self) -> TextSnapshot {
        self.text.snapshot()
    }
}
