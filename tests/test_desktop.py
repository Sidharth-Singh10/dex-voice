from dex_voice.desktop import models_ready


def test_model_check_reports_each_missing_artifact(tmp_path):
    ready, missing = models_ready(tmp_path)
    assert ready is False
    assert len(missing) == 5
    assert any("ASR encoder" in line for line in missing)


def test_model_check_requires_all_pinned_files(tmp_path):
    files = [
        "parakeet-240ms/encoder.onnx",
        "parakeet-240ms/tokens.txt",
        "phonon-31m-en/model.q8.gguf",
        "phonon-31m-en/default-voice.safetensors",
        "voices/voices.jsonl",
    ]
    for name in files:
        path = tmp_path / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"x")
    ready, missing = models_ready(tmp_path)
    assert ready is True
    assert missing == []
