//! Linux spike: Phonon through its Rust crate, on CPU, with the local checkpoint.
//!
//! Measures first PCM, total compute and generated duration. This is a
//! measurement tool, not the application.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Parser;
use ptts::checkpoint::{Checkpoint, ResolveOptions};
use ptts::preprocess::Lang;
use ptts::synth::{DeviceKind, Quant};

#[derive(Parser, Debug)]
struct Args {
    /// Local Phonon checkpoint directory.
    #[arg(long, default_value = "../models/phonon-31m-en")]
    model: PathBuf,
    /// Text group to synthesize. Repeatable.
    #[arg(long = "text", default_value = "I can hear you.")]
    texts: Vec<String>,
    /// WAV output for the last group.
    #[arg(long, default_value = "../spike-results/tts-rust.wav")]
    out: PathBuf,
    #[arg(long, default_value_t = 2)]
    repeats: u32,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let checkpoint = Checkpoint::resolve(
        &args.model,
        ResolveOptions {
            quant: Quant::Q80,
            weights: None,
        },
    )
    .with_context(|| format!("resolve {}", args.model.display()))?;
    let mut tts = checkpoint
        .builder(Lang::En)
        .device(DeviceKind::Cpu)
        .build()
        .context("build Phonon")?;
    checkpoint.register_voices(&mut tts);
    println!(
        "loaded: {} Hz, voices {:?}",
        tts.sample_rate(),
        tts.voices()
    );

    // Warm the kernels and the voice prompt outside the measured runs.
    let _ = tts.say("Ready.").context("warmup synthesis")?;

    let mut last = Vec::new();
    for _ in 0..args.repeats {
        for text in &args.texts {
            let start = Instant::now();
            let mut first = None;
            let mut samples = 0usize;
            let mut pcm = Vec::new();
            for chunk in tts
                .stream(text)
                .with_context(|| format!("stream {text:?}"))?
            {
                let chunk = chunk.context("synthesis chunk")?;
                if first.is_none() {
                    first = Some(start.elapsed().as_secs_f64() * 1000.0);
                }
                samples += chunk.len();
                if args.repeats == 1 || text == args.texts.last().unwrap() {
                    pcm.extend_from_slice(&chunk);
                }
            }
            let wall = start.elapsed().as_secs_f64() * 1000.0;
            println!(
                "{text:?}: first {:.1} ms, wall {:.1} ms, audio {:.2} s, RTF {:.2}x",
                first.unwrap_or(f64::NAN),
                wall,
                samples as f64 / tts.sample_rate() as f64,
                (samples as f64 / tts.sample_rate() as f64) / (wall / 1000.0)
            );
            last = pcm;
        }
    }

    if let Some(parent) = args.out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    ptts::wav::write_wav_file(&args.out, &last, tts.sample_rate())
        .with_context(|| format!("write {}", args.out.display()))?;
    println!(
        "wrote {} ({:.2} s)",
        args.out.display(),
        last.len() as f64 / tts.sample_rate() as f64
    );
    Ok(())
}
