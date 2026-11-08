//! Local settings and model paths. Linux: XDG; Windows: %LOCALAPPDATA%.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub asr_model: String,
    pub tts_model: String,
    pub voice: String,
    pub device: String,
    pub input_device: String,
    pub output_device: String,
    pub group_words: usize,
    pub group_ms: u64,
    pub trim_tts: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let models = default_models_dir();
        Self {
            asr_model: models
                .join("gguf/parakeet-unified-en-0.6b-F16.gguf")
                .display()
                .to_string(),
            tts_model: models.join("phonon-31m-en").display().to_string(),
            voice: "default".into(),
            device: "nvidia".into(),
            input_device: String::new(),
            output_device: String::new(),
            group_words: 3,
            group_ms: 240,
            trim_tts: true,
        }
    }
}

pub fn data_dir() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("DexVoice")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .unwrap_or_else(|| PathBuf::from("."))
            .join("dex-voice")
    }
}

/// During development, prefer the checkout's models when `DEX_MODELS` is set.
pub fn default_models_dir() -> PathBuf {
    std::env::var_os("DEX_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir().join("models"))
}

pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

pub fn load() -> Settings {
    match std::fs::read_to_string(settings_path()) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

pub fn save(settings: &Settings) -> std::io::Result<()> {
    // Tests must not write the developer's real settings file. A test that
    // genuinely exercises persistence can opt in with this variable.
    #[cfg(test)]
    if std::env::var_os("DEX_TEST_ALLOW_SETTINGS_WRITE").is_none() {
        return Ok(());
    }
    let path = settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(settings).unwrap_or_default();
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, text + "\n")?;
    std::fs::rename(temporary, path)
}

/// Voice files beside the selected local checkpoint; catalog metadata without
/// downloaded embeddings is deliberately not shown as an available voice.
pub fn available_voices(checkpoint: &std::path::Path) -> Vec<String> {
    let mut voices = Vec::new();
    if checkpoint.join("default-voice.safetensors").is_file() {
        voices.push("default".to_owned());
    }
    for subdir in ["voices", "embeddings"] {
        if let Ok(entries) = std::fs::read_dir(checkpoint.join(subdir)) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "safetensors")
                    && let Some(name) = path.file_stem().and_then(|stem| stem.to_str())
                    && !voices.iter().any(|v| v == name)
                {
                    voices.push(name.to_owned());
                }
            }
        }
    }
    if voices.len() > 1 {
        let start = usize::from(voices.first().is_some_and(|v| v == "default"));
        voices[start..].sort();
    }
    voices
}

pub fn voice_label(id: &str) -> &str {
    match id {
        "NbpkqMVS3CJeq2j8" => "Zoey",
        "NIWIOv5vTi5eRMj8" => "Juniper",
        "default" => "Default",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn available_voices_only_lists_installed_checkpoint_assets() {
        let dir = std::env::temp_dir().join(format!("dex-voices-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("voices")).unwrap();
        std::fs::write(dir.join("default-voice.safetensors"), b"voice").unwrap();
        std::fs::write(dir.join("voices/alto.safetensors"), b"voice").unwrap();
        std::fs::write(dir.join("voices/ignore.txt"), b"not a voice").unwrap();
        assert_eq!(available_voices(&dir), vec!["default", "alto"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn selected_voice_is_preserved_in_settings_json() {
        let settings = Settings {
            voice: "alto".into(),
            ..Settings::default()
        };
        let loaded: Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(loaded.voice, "alto");
    }

    #[test]
    fn installed_catalog_voice_has_a_readable_label() {
        assert_eq!(voice_label("NIWIOv5vTi5eRMj8"), "Juniper");
        assert_eq!(voice_label("custom"), "custom");
    }
}
