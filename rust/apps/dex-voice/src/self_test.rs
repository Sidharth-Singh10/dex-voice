//! Device-free fixture test: exercise the same workers used by the desktop app.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use dex_audio::PcmQueue;
use dex_core::{Engine, EngineConfig, State};
use serde_json::{Value, json};

use crate::config::Settings;

pub fn validate(settings: &Settings) -> Result<()> {
    if !Path::new(&settings.asr_model).is_file() {
        bail!("Parakeet F16 GGUF not found: {}", settings.asr_model);
    }
    if !Path::new(&settings.tts_model).join("config.json").is_file() {
        bail!("Phonon checkpoint not found: {}", settings.tts_model);
    }
    if !crate::config::available_voices(Path::new(&settings.tts_model)).contains(&settings.voice) {
        bail!(
            "voice {:?} is not installed in the selected checkpoint",
            settings.voice
        );
    }
    Ok(())
}

pub fn run(out: &Path) -> Result<()> {
    let result = run_inner();
    let report = match &result {
        Ok(value) => value.clone(),
        Err(error) => json!({"ok": false, "error": format!("{error:#}")}),
    };
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(out, serde_json::to_vec_pretty(&report)?)?;
    println!("self-test report: {}", out.display());
    result.map(|_| ())
}

fn run_inner() -> Result<Value> {
    let settings = crate::config::load();
    validate(&settings)?;
    let fixture = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../models/parakeet-240ms/test_wavs/0.wav"
    ));
    let mut reader = hound::WavReader::new(Cursor::new(fixture))?;
    let spec = reader.spec();
    if spec.sample_rate != 16_000 || spec.channels != 1 || spec.bits_per_sample != 16 {
        bail!("embedded self-test fixture must be 16 kHz mono PCM16");
    }
    let samples = reader
        .samples::<i16>()
        .map(|sample| sample.map(|v| v as f32 / 32768.0))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let rate = 48_000;
    let output = Arc::new(PcmQueue::new(rate as usize * 30));
    let engine = Engine::start(
        EngineConfig {
            asr_model: PathBuf::from(settings.asr_model),
            tts_model: PathBuf::from(settings.tts_model),
            voice: settings.voice,
            device: settings.device,
            group_words: settings.group_words,
            group_ms: settings.group_ms,
            trim_tts: settings.trim_tts,
            output_rate: rate,
            ..EngineConfig::default()
        },
        Arc::clone(&output),
    )?;
    let ready_at = Instant::now();
    loop {
        let status = engine.status();
        match status.state {
            State::Running => break,
            State::Failed => bail!("{}", status.error.unwrap_or_default()),
            _ if ready_at.elapsed() > Duration::from_secs(120) => {
                bail!("models did not become ready")
            }
            _ => thread::sleep(Duration::from_millis(100)),
        }
    }
    let mut block = vec![0.0; 960];
    let mut peak = 0.0f32;
    let mut played = 0usize;
    let start = Instant::now();
    for (index, chunk) in samples.chunks(320).enumerate() {
        let due = Duration::from_millis(index as u64 * 20);
        if let Some(wait) = due.checked_sub(start.elapsed()) {
            thread::sleep(wait);
        }
        engine.push_input(chunk);
        played += output.pop_into(&mut block);
        peak = peak.max(block.iter().fold(0.0f32, |p, v| p.max(v.abs())));
    }
    // Keep the input channel alive long enough for the silence finalizer to
    // commit the last word, then stop and drain the generated PCM.
    for _ in 0..150 {
        thread::sleep(Duration::from_millis(20));
        played += output.pop_into(&mut block);
        peak = peak.max(block.iter().fold(0.0f32, |p, v| p.max(v.abs())));
    }
    engine.stop();
    while !output.is_empty() {
        played += output.pop_into(&mut block);
        peak = peak.max(block.iter().fold(0.0f32, |p, v| p.max(v.abs())));
    }
    let status = engine.status();
    if let Some(error) = status.error {
        bail!("{error}");
    }
    let ok = status.transcript.to_lowercase().contains("portrait")
        && peak > 0.01
        && played > 0
        && output.dropped() == 0
        && status.input_drops == 0;
    let report = json!({
        "ok": ok,
        "transcript": status.transcript.trim(),
        "words": status.words,
        "pcm_samples": played,
        "peak": peak,
        "input_drops": status.input_drops,
        "output_dropped_samples": output.dropped(),
        "group_p50_ms": status.p50_ms,
    });
    if !ok {
        bail!("self-test did not recognize the final word and generate audible PCM: {report}");
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_models_are_reported_without_starting_gpu() {
        let settings = Settings {
            asr_model: "/no/such/parakeet.gguf".into(),
            tts_model: "/no/such/phonon".into(),
            ..Settings::default()
        };
        assert!(validate(&settings).is_err());
    }
}
