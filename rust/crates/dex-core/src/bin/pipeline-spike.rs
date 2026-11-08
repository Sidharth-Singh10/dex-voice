//! File-paced end-to-end run: WAV → ASR → Phonon → simulated playback.
//!
//! The simulated player drains the output queue at real time, so overflow and
//! underrun are visible. This is the engine validation the Rune UI will use.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::Parser;
use dex_audio::PcmQueue;
use dex_core::{Engine, EngineConfig, State};

#[derive(Parser, Debug)]
struct Args {
    #[arg(
        long,
        default_value = "../models/gguf/parakeet-unified-en-0.6b-F16.gguf"
    )]
    asr_model: PathBuf,
    #[arg(long, default_value = "../models/phonon-31m-en")]
    tts_model: PathBuf,
    #[arg(long, default_value = "../models/parakeet-240ms/test_wavs/0.wav")]
    wav: PathBuf,
    #[arg(long, default_value = "../spike-results/pipeline-rust.wav")]
    out: PathBuf,
    #[arg(long, default_value = "nvidia")]
    device: String,
    #[arg(long, default_value_t = 160)]
    chunk_ms: i32,
    #[arg(long, default_value_t = 80)]
    right_ms: i32,
    #[arg(long, default_value_t = 3)]
    group_words: usize,
    #[arg(long, default_value_t = 240)]
    group_ms: u64,
    #[arg(long, default_value_t = false)]
    no_trim: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let (pcm, rate) = read_wav(&args.wav)?;
    if rate != 16_000 {
        bail!("expected 16 kHz input, got {rate}");
    }
    let output_rate = 48_000u32;
    let queue = Arc::new(PcmQueue::new(output_rate as usize * 2));
    let config = EngineConfig {
        asr_model: args.asr_model,
        tts_model: args.tts_model,
        device: args.device,
        group_words: args.group_words,
        group_ms: args.group_ms,
        trim_tts: !args.no_trim,
        output_rate,
        chunk_ms: args.chunk_ms,
        right_ms: args.right_ms,
        ..EngineConfig::default()
    };

    let started = Instant::now();
    let engine = Engine::start(config, Arc::clone(&queue)).context("start engine")?;
    wait_ready(&engine, Duration::from_secs(120))?;
    println!("engine ready in {:.2} s", started.elapsed().as_secs_f64());

    // Simulated playback: drain two seconds of output at real time.
    let stop = Arc::new(AtomicBool::new(false));
    let collected = Arc::new(Mutex::new(Vec::<f32>::new()));
    let player = {
        let queue = Arc::clone(&queue);
        let stop = Arc::clone(&stop);
        let collected = Arc::clone(&collected);
        thread::spawn(move || {
            let mut block = vec![0.0f32; output_rate as usize / 50];
            while !stop.load(Ordering::Relaxed) {
                queue.pop_into(&mut block);
                collected
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .extend_from_slice(&block);
                thread::sleep(Duration::from_millis(20));
            }
        })
    };

    let chunk = 320usize;
    let start = Instant::now();
    for (index, block) in pcm.chunks(chunk).enumerate() {
        let due = Duration::from_millis(index as u64 * 20);
        let now = start.elapsed();
        if due > now {
            thread::sleep(due - now);
        }
        engine.push_input(block);
    }
    // Let the tail synthesize before stopping.
    thread::sleep(Duration::from_secs(2));
    engine.stop();
    stop.store(true, Ordering::Relaxed);
    player.join().ok();

    // Drain whatever the TTS thread queued while the simulated player was
    // stopping, so the WAV contains the complete output.
    let mut tail = vec![0.0f32; output_rate as usize / 50];
    while !queue.is_empty() {
        queue.pop_into(&mut tail);
        collected
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend_from_slice(&tail);
    }

    let status = engine.status();
    let audio = collected.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(parent) = args.out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    ptts::wav::write_wav_file(&args.out, &audio, output_rate)
        .with_context(|| format!("write {}", args.out.display()))?;
    println!(
        "wrote {} ({:.2} s), transcript: {}",
        args.out.display(),
        audio.len() as f64 / output_rate as f64,
        status.transcript
    );
    println!(
        "words={} p50={:?} p95={:?} input_drops={} dropped_seconds={:.2} error={:?}",
        status.words,
        status.p50_ms,
        status.p95_ms,
        status.input_drops,
        status.dropped_seconds,
        status.error
    );
    if let Some(error) = status.error {
        bail!("engine failed: {error}");
    }
    Ok(())
}

fn wait_ready(engine: &Engine, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        let status = engine.status();
        match status.state {
            State::Running => return Ok(()),
            State::Failed => bail!("engine failed: {}", status.error.unwrap_or_default()),
            _ if Instant::now() >= deadline => bail!("engine did not become ready"),
            _ => thread::sleep(Duration::from_millis(100)),
        }
    }
}

fn read_wav(path: &std::path::Path) -> Result<(Vec<f32>, u32)> {
    let mut reader =
        hound::WavReader::open(path).with_context(|| format!("open {}", path.display()))?;
    let spec = reader.spec();
    let pcm = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, _) => reader
            .samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>()?,
        (hound::SampleFormat::Int, 16) => reader
            .samples::<i16>()
            .map(|v| v.map(|s| s as f32 / 32768.0))
            .collect::<std::result::Result<Vec<_>, _>>()?,
        other => bail!("unsupported WAV format {other:?}"),
    };
    Ok((pcm, spec.sample_rate))
}
