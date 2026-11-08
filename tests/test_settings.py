from pathlib import Path

import pytest

from dex_voice.settings import Settings, data_directory, load_settings, save_settings


def test_windows_data_is_outside_the_executable_directory(monkeypatch, tmp_path):
    monkeypatch.setattr("sys.platform", "win32")
    monkeypatch.setenv("LOCALAPPDATA", str(tmp_path))
    assert data_directory() == tmp_path / "DexVoice"


def test_missing_settings_uses_safe_defaults_without_microphone_autostart(tmp_path):
    settings = load_settings(tmp_path / "settings.json")
    assert settings.voice == "default"
    assert settings.trim_tts is False
    assert settings.input_name == ""
    assert settings.output_name == ""


def test_settings_roundtrip_and_custom_model_selection(tmp_path):
    path = tmp_path / "settings.json"
    settings = Settings(
        models=str(tmp_path / "models"), tts_model=str(tmp_path / "custom"), voice="Zoey"
    )
    save_settings(path, settings)
    actual = load_settings(path)
    assert actual == settings
    assert actual.tts_directory == tmp_path / "custom"


def test_default_model_follows_selected_model_cache_directory(tmp_path):
    assert Settings(models=str(tmp_path)).tts_directory == tmp_path / "phonon-31m-en"


def test_invalid_settings_are_reported_not_silently_rewritten(tmp_path):
    path = tmp_path / "settings.json"
    path.write_text('{"trim_tts": "false"}', encoding="utf-8")
    with pytest.raises(ValueError, match="settings"):
        load_settings(path)
    assert path.read_text(encoding="utf-8") == '{"trim_tts": "false"}'


def test_model_cache_path_remains_an_absolute_user_choice(tmp_path):
    settings = Settings(models=str(tmp_path))
    assert Path(settings.models).is_absolute()
