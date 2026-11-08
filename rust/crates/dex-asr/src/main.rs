//! Linux spike: Parakeet Unified streaming through transcribe.cpp on Vulkan.
//!
//! Offline run + paced streaming run, with transcript parity against the
//! Python reference and per-word commit latency. This is a measurement tool,
//! not the application.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use transcribe_cpp::{
    Backend, CommitPolicy, Device, DeviceType, Model, ModelOptions, ParakeetBufferedStreamOptions,
    RunOptions, StreamExtension, StreamOptions, TimestampKind, Token, devices,
    init_backends_default,
};

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    model: PathBuf,
    #[arg(long)]
    wav: PathBuf,
    #[arg(long)]
    reference: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = Mode::Both)]
    mode: Mode,
    /// Real-time feed cadence.
    #[arg(long, default_value_t = 20)]
    feed_ms: u64,
    /// Parakeet buffered-streaming left context.
    #[arg(long, default_value_t = 5600)]
    left_ms: i32,
    /// Parakeet buffered-streaming chunk.
    #[arg(long, default_value_t = 80)]
    stream_chunk_ms: i32,
    /// Parakeet buffered-streaming right context.
    #[arg(long, default_value_t = 160)]
    right_ms: i32,
    /// Consecutive agreeing hypotheses before a prefix commits (0 = library default 3).
    #[arg(long, default_value_t = 0)]
    commit_agreement: u32,
    #[arg(long, value_enum, default_value_t = CommitArg::Auto)]
    commit_policy: CommitArg,
    /// Use committed text or early token rows to detect word boundaries.
    #[arg(long, value_enum, default_value_t = SourceArg::Text)]
    source: SourceArg,
    /// Device name substring. Empty selects the first discrete GPU.
    #[arg(long, default_value = "nvidia")]
    device: String,
    #[arg(long, value_enum, default_value_t = BackendArg::Vulkan)]
    backend: BackendArg,
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Mode {
    Offline,
    Stream,
    Both,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum BackendArg {
    Vulkan,
    Cuda,
}

impl BackendArg {
    fn to_backend(self) -> Backend {
        match self {
            BackendArg::Vulkan => Backend::Vulkan,
            BackendArg::Cuda => Backend::Cuda,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            BackendArg::Vulkan => "vulkan",
            BackendArg::Cuda => "cuda",
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum SourceArg {
    Text,
    Full,
    Tokens,
}

impl SourceArg {
    fn name(self) -> &'static str {
        match self {
            SourceArg::Text => "text",
            SourceArg::Full => "full",
            SourceArg::Tokens => "tokens",
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CommitArg {
    Auto,
    Stable,
    Final,
}

impl CommitArg {
    fn to_policy(self) -> CommitPolicy {
        match self {
            CommitArg::Auto => CommitPolicy::Auto,
            CommitArg::Stable => CommitPolicy::StablePrefix,
            CommitArg::Final => CommitPolicy::OnFinalize,
        }
    }

    fn name(self) -> &'static str {
        match self {
            CommitArg::Auto => "auto",
            CommitArg::Stable => "stable",
            CommitArg::Final => "final",
        }
    }
}

#[derive(Debug, Deserialize)]
struct Reference {
    words: Vec<RefWord>,
}

#[derive(Debug, Deserialize)]
struct RefWord {
    word: String,
    #[allow(dead_code)]
    start: f64,
    end: f64,
}

#[derive(Debug, Serialize)]
struct DeviceInfo {
    name: String,
    description: String,
    kind: String,
    device_type: String,
    memory_total_mib: u64,
    memory_free_mib: u64,
}

impl From<&Device> for DeviceInfo {
    fn from(device: &Device) -> Self {
        Self {
            name: device.name.clone(),
            description: device.description.clone(),
            kind: device.kind.clone(),
            device_type: format!("{:?}", device.device_type),
            memory_total_mib: device.memory_total / (1024 * 1024),
            memory_free_mib: device.memory_free / (1024 * 1024),
        }
    }
}

#[derive(Debug, Serialize)]
struct CapabilityInfo {
    sample_rate: i32,
    supports_streaming: bool,
    max_audio_ms: i64,
    languages: Vec<String>,
}

#[derive(Debug, Serialize)]
struct OfflineReport {
    text: String,
    audio_ms: f64,
    wall_ms: f64,
    rtf: f64,
    mel_ms: f32,
    encode_ms: f32,
    decode_ms: f32,
    word_error_rate: Option<f64>,
    words: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CommitEvent {
    index: usize,
    word: String,
    wall_ms: f64,
    input_ms: i64,
    ref_end_ms: Option<f64>,
    delay_ms: Option<f64>,
}

#[derive(Debug, Serialize)]
struct Latency {
    count: usize,
    p50_ms: Option<f64>,
    p95_ms: Option<f64>,
    max_ms: Option<f64>,
    misses_500ms: usize,
}

#[derive(Debug, Serialize)]
struct StreamReport {
    left_ms: i32,
    chunk_ms: i32,
    right_ms: i32,
    commit_policy: String,
    commit_agreement: u32,
    source: String,
    text: String,
    audio_ms: f64,
    wall_ms: f64,
    rtf: f64,
    events: Vec<CommitEvent>,
    latency: Latency,
    stt_within_500ms: Option<bool>,
    word_error_rate: Option<f64>,
}

#[derive(Debug, Serialize)]
struct Report {
    model: String,
    wav: String,
    backend: String,
    device: DeviceInfo,
    capabilities: CapabilityInfo,
    reference_words: usize,
    gpu_peak_mib: Option<u32>,
    offline: Option<OfflineReport>,
    stream: Option<StreamReport>,
}

struct GpuSampler {
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<Vec<u32>>>,
}

impl GpuSampler {
    fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            let mut samples = Vec::new();
            while !flag.load(Ordering::Relaxed) {
                if let Ok(output) = std::process::Command::new("nvidia-smi")
                    .args(["--query-gpu=memory.used", "--format=csv,noheader,nounits"])
                    .output()
                {
                    for line in String::from_utf8_lossy(&output.stdout).lines() {
                        if let Ok(value) = line.trim().parse::<u32>() {
                            samples.push(value);
                        }
                    }
                }
                thread::sleep(Duration::from_millis(200));
            }
            samples
        });
        Self {
            stop,
            handle: Some(handle),
        }
    }

    fn finish(mut self) -> Option<u32> {
        self.stop.store(true, Ordering::Relaxed);
        let samples = self.handle.take()?.join().ok()?;
        samples.into_iter().max()
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.feed_ms == 0 {
        bail!("--feed-ms must be positive");
    }

    init_backends_default().context("initialise transcribe.cpp backends")?;
    let available = devices();
    if available.is_empty() {
        bail!("no compute devices registered");
    }
    println!("devices:");
    for device in &available {
        println!(
            "  [{}] kind={} type={:?} {} — {} ({} MiB free / {} MiB)",
            device.index.unwrap_or_default(),
            device.kind,
            device.device_type,
            device.name,
            device.description,
            device.memory_free / (1024 * 1024),
            device.memory_total / (1024 * 1024)
        );
    }

    let device = select_device(&available, &args.device)?;
    println!(
        "selected: {} ({}) on {}",
        device.description,
        device.kind,
        args.backend.as_str()
    );

    let pcm = read_wav(&args.wav)?;
    let audio_ms = pcm.len() as f64 / 16.0;
    let reference = args
        .reference
        .as_ref()
        .map(|path| read_reference(path))
        .transpose()?;

    let sampler = GpuSampler::start();
    let load_start = Instant::now();
    let model = Model::load_with(
        &args.model,
        &ModelOptions {
            backend: args.backend.to_backend(),
            device: Some(device.clone()),
            no_mmap: true,
        },
    )
    .with_context(|| format!("load {}", args.model.display()))?;
    let load_ms = load_start.elapsed().as_secs_f64() * 1000.0;
    let caps = model.capabilities().context("read model capabilities")?;
    println!(
        "loaded in {load_ms:.0} ms: streaming={} rate={} Hz",
        caps.supports_streaming, caps.native_sample_rate
    );
    if !caps.supports_streaming {
        bail!("this model does not advertise streaming");
    }

    let mut session = model.session().context("create session")?;
    let run_options = RunOptions {
        timestamps: TimestampKind::Token,
        ..RunOptions::default()
    };

    // Warm the Vulkan pipelines outside the measured runs.
    {
        let mut warm = session
            .stream(
                &run_options,
                &StreamOptions {
                    commit_policy: args.commit_policy.to_policy(),
                    stable_prefix_agreement_n: args.commit_agreement,
                    family: Some(StreamExtension::ParakeetBuffered(
                        ParakeetBufferedStreamOptions {
                            left_ms: Some(args.left_ms),
                            chunk_ms: Some(args.stream_chunk_ms),
                            right_ms: Some(args.right_ms),
                        },
                    )),
                },
            )
            .context("create warmup stream")?;
        warm.feed(&vec![0.0f32; 8000]).context("warmup feed")?;
        warm.finalize().context("warmup finalize")?;
    }

    let mut offline = None;
    if matches!(args.mode, Mode::Offline | Mode::Both) {
        let start = Instant::now();
        let transcript = session
            .run(&pcm, &run_options)
            .context("offline transcription")?;
        let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
        let words = transcript
            .words
            .iter()
            .map(|word| word.text.clone())
            .collect::<Vec<_>>();
        let wer = reference.as_ref().map(|reference| {
            let actual: Vec<String> = transcript
                .text
                .split_whitespace()
                .map(str::to_string)
                .collect();
            align(&actual, reference).1 as f64 / reference.words.len().max(1) as f64
        });
        println!(
            "\noffline: {:.0} ms for {:.0} ms audio ({:.2}x real time)",
            wall_ms,
            audio_ms,
            audio_ms / wall_ms
        );
        println!("text: {}", transcript.text);
        if let Some(wer) = wer {
            println!("WER: {:.2}%", wer * 100.0);
        }
        offline = Some(OfflineReport {
            text: transcript.text,
            audio_ms,
            wall_ms,
            rtf: audio_ms / wall_ms,
            mel_ms: transcript.timings.mel_ms,
            encode_ms: transcript.timings.encode_ms,
            decode_ms: transcript.timings.decode_ms,
            word_error_rate: wer,
            words,
        });
    }

    let mut stream_report = None;
    if matches!(args.mode, Mode::Stream | Mode::Both) {
        let stream_options = StreamOptions {
            commit_policy: args.commit_policy.to_policy(),
            stable_prefix_agreement_n: args.commit_agreement,
            family: Some(StreamExtension::ParakeetBuffered(
                ParakeetBufferedStreamOptions {
                    left_ms: Some(args.left_ms),
                    chunk_ms: Some(args.stream_chunk_ms),
                    right_ms: Some(args.right_ms),
                },
            )),
        };
        let mut stream = session
            .stream(&run_options, &stream_options)
            .context("start stream")?;

        let chunk_samples = (args.feed_ms as usize) * 16;
        let start = Instant::now();
        let mut seen_words = 0usize;
        let mut events = Vec::new();
        let mut tokens = TokenAssembler::default();

        for (index, chunk) in pcm.chunks(chunk_samples).enumerate() {
            let due = Duration::from_millis(index as u64 * args.feed_ms);
            let now = start.elapsed();
            if due > now {
                thread::sleep(due - now);
            }
            let update = stream.feed(chunk).context("stream feed")?;
            let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
            match args.source {
                SourceArg::Text if update.committed_changed => {
                    collect_words(
                        &stream.text().committed,
                        &mut seen_words,
                        wall_ms,
                        update.input_received_ms,
                        &mut events,
                    );
                }
                SourceArg::Tokens if update.result_changed => {
                    let snapshot = stream.snapshot();
                    tokens.push(
                        &snapshot.tokens,
                        wall_ms,
                        update.input_received_ms,
                        &mut events,
                    );
                }
                SourceArg::Full if update.result_changed => {
                    collect_words(
                        &stream.text().full,
                        &mut seen_words,
                        wall_ms,
                        update.input_received_ms,
                        &mut events,
                    );
                }
                _ => {}
            }
        }
        let final_update = stream.finalize().context("stream finalize")?;
        let final_text = stream.text();
        let text = final_text.committed;
        let full = final_text.full;
        match args.source {
            SourceArg::Tokens => tokens.finish(
                start.elapsed().as_secs_f64() * 1000.0,
                final_update.input_received_ms,
                &mut events,
            ),
            SourceArg::Text => collect_all_final(
                &text,
                &mut seen_words,
                start.elapsed().as_secs_f64() * 1000.0,
                final_update.input_received_ms,
                &mut events,
            ),
            SourceArg::Full => collect_all_final(
                &full,
                &mut seen_words,
                start.elapsed().as_secs_f64() * 1000.0,
                final_update.input_received_ms,
                &mut events,
            ),
        }
        let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
        let final_words: Vec<String> = match args.source {
            SourceArg::Tokens => tokens.words.clone(),
            SourceArg::Text => text.split_whitespace().map(str::to_string).collect(),
            SourceArg::Full => full.split_whitespace().map(str::to_string).collect(),
        };
        let (mapping, edit_distance) = match reference.as_ref() {
            Some(reference) => align(&final_words, reference),
            None => (vec![None; final_words.len()], 0),
        };
        for event in &mut events {
            if let Some(Some(ref_index)) = mapping.get(event.index) {
                let end = reference.as_ref().unwrap().words[*ref_index].end * 1000.0;
                event.ref_end_ms = Some(end);
                event.delay_ms = Some(event.wall_ms - end);
            }
        }
        let latency = summarize(&events);
        let wer = reference
            .as_ref()
            .map(|reference| edit_distance as f64 / reference.words.len().max(1) as f64);
        println!(
            "\nstream ({}/{}/{} ms): {:.0} ms for {:.0} ms audio ({:.2}x real time)",
            args.left_ms,
            args.stream_chunk_ms,
            args.right_ms,
            wall_ms,
            audio_ms,
            audio_ms / wall_ms
        );
        println!("text: {text}");
        if let Some(latency) = &latency {
            println!(
                "commit delay p50 {:.0} ms, p95 {:.0} ms, max {:.0} ms, misses>=500ms: {}",
                latency.p50_ms.unwrap_or(f64::NAN),
                latency.p95_ms.unwrap_or(f64::NAN),
                latency.max_ms.unwrap_or(f64::NAN),
                latency.misses_500ms
            );
        }
        if let Some(wer) = wer {
            println!("WER: {:.2}%", wer * 100.0);
        }
        stream_report = Some(StreamReport {
            left_ms: args.left_ms,
            chunk_ms: args.stream_chunk_ms,
            right_ms: args.right_ms,
            commit_policy: args.commit_policy.name().to_string(),
            commit_agreement: args.commit_agreement,
            source: args.source.name().to_string(),
            text,
            audio_ms,
            wall_ms,
            rtf: audio_ms / wall_ms,
            events,
            stt_within_500ms: latency
                .as_ref()
                .map(|value| value.misses_500ms == 0 && wer == Some(0.0)),
            latency: latency.unwrap_or(Latency {
                count: 0,
                p50_ms: None,
                p95_ms: None,
                max_ms: None,
                misses_500ms: 0,
            }),
            word_error_rate: wer,
        });
    }

    drop(session);
    drop(model);
    let gpu_peak_mib = sampler.finish();

    let report = Report {
        model: args.model.display().to_string(),
        wav: args.wav.display().to_string(),
        backend: args.backend.as_str().to_string(),
        device: DeviceInfo::from(&device),
        capabilities: CapabilityInfo {
            sample_rate: caps.native_sample_rate,
            supports_streaming: caps.supports_streaming,
            max_audio_ms: caps.max_audio_ms,
            languages: caps.languages.clone(),
        },
        reference_words: reference.as_ref().map_or(0, |value| value.words.len()),
        gpu_peak_mib,
        offline,
        stream: stream_report,
    };

    if let Some(path) = &args.out {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&report)?)
            .with_context(|| format!("write {}", path.display()))?;
        println!("\nreport: {}", path.display());
    }
    Ok(())
}

#[derive(Default)]
struct TokenAssembler {
    pending: String,
    seen: usize,
    index: usize,
    words: Vec<String>,
}

impl TokenAssembler {
    fn push(
        &mut self,
        tokens: &[Token],
        wall_ms: f64,
        input_ms: i64,
        events: &mut Vec<CommitEvent>,
    ) {
        if tokens.len() < self.seen {
            self.pending.clear();
            self.seen = 0;
            self.index = 0;
        }
        for token in &tokens[self.seen..] {
            let piece = token.text.as_str();
            if piece.is_empty() || (piece.starts_with('<') && piece.ends_with('>')) {
                continue;
            }
            let starts_word = piece.starts_with('▁');
            let text = piece.strip_prefix('▁').unwrap_or(piece);
            let punctuation = !text.chars().any(|character| character.is_alphanumeric());
            if punctuation {
                if !self.pending.is_empty() {
                    self.pending.push_str(text);
                }
                continue;
            }
            if starts_word && !self.pending.is_empty() {
                self.emit(wall_ms, input_ms, events);
            }
            self.pending.push_str(text);
        }
        self.seen = tokens.len();
    }

    fn finish(&mut self, wall_ms: f64, input_ms: i64, events: &mut Vec<CommitEvent>) {
        if !self.pending.is_empty() {
            self.emit(wall_ms, input_ms, events);
        }
    }

    fn emit(&mut self, wall_ms: f64, input_ms: i64, events: &mut Vec<CommitEvent>) {
        self.words.push(self.pending.clone());
        push_event(self.index, &self.pending, wall_ms, input_ms, events);
        self.index += 1;
        self.pending.clear();
    }
}

fn select_device(devices: &[Device], wanted: &str) -> Result<Device> {
    let wanted = wanted.trim().to_lowercase();
    let matching: Vec<&Device> = devices
        .iter()
        .filter(|device| device.device_type != DeviceType::Cpu)
        .filter(|device| {
            wanted.is_empty()
                || device.description.to_lowercase().contains(&wanted)
                || device.name.to_lowercase().contains(&wanted)
        })
        .collect();
    if matching.is_empty() {
        bail!(
            "no GPU device matching {wanted:?}; available: {}",
            devices
                .iter()
                .map(|device| format!("{} ({})", device.description, device.kind))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if matching.len() > 1 {
        println!(
            "warning: {} devices match {:?}; using {}",
            matching.len(),
            wanted,
            matching[0].description
        );
    }
    Ok(matching[0].clone())
}

fn read_wav(path: &Path) -> Result<Vec<f32>> {
    let mut reader =
        hound::WavReader::open(path).with_context(|| format!("open {}", path.display()))?;
    let spec = reader.spec();
    if spec.channels != 1 {
        bail!("expected mono WAV, got {} channels", spec.channels);
    }
    if spec.sample_rate != 16_000 {
        bail!("expected 16 kHz WAV, got {} Hz", spec.sample_rate);
    }
    let pcm = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, _) => reader
            .samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>()?,
        (hound::SampleFormat::Int, 16) => reader
            .samples::<i16>()
            .map(|value| value.map(|sample| sample as f32 / 32768.0))
            .collect::<std::result::Result<Vec<_>, _>>()?,
        (hound::SampleFormat::Int, 32) => reader
            .samples::<i32>()
            .map(|value| value.map(|sample| sample as f32 / 2147483648.0))
            .collect::<std::result::Result<Vec<_>, _>>()?,
        (format, bits) => bail!("unsupported WAV format {format:?} with {bits} bits"),
    };
    Ok(pcm)
}

fn read_reference(path: &Path) -> Result<Reference> {
    let text = std::fs::read_to_string(path).with_context(|| format!("open {}", path.display()))?;
    let reference: Reference =
        serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    if reference.words.is_empty() {
        bail!("reference has no words");
    }
    Ok(reference)
}

fn normalize(word: &str) -> String {
    word.chars()
        .filter(|character| character.is_alphanumeric() || *character == '\'')
        .flat_map(|character| character.to_lowercase())
        .collect()
}

fn collect_words(
    text: &str,
    seen: &mut usize,
    wall_ms: f64,
    input_ms: i64,
    events: &mut Vec<CommitEvent>,
) {
    let words: Vec<&str> = text.split_whitespace().collect();
    // The last whitespace token may still grow; commit it only when the
    // generator closed it (trailing whitespace) — finalization handles the rest.
    let complete = if text.chars().last().is_some_and(char::is_whitespace) {
        words.len()
    } else {
        words.len().saturating_sub(1)
    };
    if complete <= *seen {
        return;
    }
    for (offset, word) in words[*seen..complete].iter().enumerate() {
        push_event(*seen + offset, word, wall_ms, input_ms, events);
    }
    *seen = complete;
}

fn collect_all_final(
    text: &str,
    seen: &mut usize,
    wall_ms: f64,
    input_ms: i64,
    events: &mut Vec<CommitEvent>,
) {
    let words: Vec<&str> = text.split_whitespace().collect();
    for (offset, word) in words[*seen..].iter().enumerate() {
        push_event(*seen + offset, word, wall_ms, input_ms, events);
    }
    *seen = words.len();
}

fn push_event(
    index: usize,
    word: &str,
    wall_ms: f64,
    input_ms: i64,
    events: &mut Vec<CommitEvent>,
) {
    if normalize(word).is_empty() {
        return;
    }
    events.push(CommitEvent {
        index,
        word: word.to_string(),
        wall_ms,
        input_ms,
        ref_end_ms: None,
        delay_ms: None,
    });
}

fn summarize(events: &[CommitEvent]) -> Option<Latency> {
    let mut delays: Vec<f64> = events.iter().filter_map(|event| event.delay_ms).collect();
    if delays.is_empty() {
        return None;
    }
    delays.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pick = |fraction: f64| {
        let index = ((delays.len() - 1) as f64 * fraction).round() as usize;
        delays[index]
    };
    Some(Latency {
        count: delays.len(),
        p50_ms: Some(pick(0.50)),
        p95_ms: Some(pick(0.95)),
        max_ms: delays.last().copied(),
        misses_500ms: delays.iter().filter(|value| **value >= 500.0).count(),
    })
}

fn align(actual: &[String], reference: &Reference) -> (Vec<Option<usize>>, usize) {
    let expected: Vec<String> = reference
        .words
        .iter()
        .map(|word| normalize(&word.word))
        .collect();
    let normalized: Vec<String> = actual.iter().map(|word| normalize(word)).collect();
    let mut costs = vec![vec![0usize; normalized.len() + 1]; expected.len() + 1];
    for (index, row) in costs.iter_mut().enumerate() {
        row[0] = index;
    }
    for (index, cell) in costs[0].iter_mut().enumerate() {
        *cell = index;
    }
    for i in 1..=expected.len() {
        for j in 1..=normalized.len() {
            let substitution =
                costs[i - 1][j - 1] + usize::from(expected[i - 1] != normalized[j - 1]);
            costs[i][j] = substitution
                .min(costs[i - 1][j] + 1)
                .min(costs[i][j - 1] + 1);
        }
    }
    let mut mapping = vec![None; normalized.len()];
    let (mut i, mut j) = (expected.len(), normalized.len());
    while i > 0 || j > 0 {
        if i > 0
            && j > 0
            && costs[i][j]
                == costs[i - 1][j - 1] + usize::from(expected[i - 1] != normalized[j - 1])
        {
            if expected[i - 1] == normalized[j - 1] {
                mapping[j - 1] = Some(i - 1);
            }
            i -= 1;
            j -= 1;
        } else if i > 0 && costs[i][j] == costs[i - 1][j] + 1 {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    (mapping, costs[expected.len()][normalized.len()])
}
