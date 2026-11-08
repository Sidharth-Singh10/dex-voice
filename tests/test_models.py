import json

import pytest

from dex_voice.models import catalog, register_voice


def test_catalog_requires_explicit_model_download(tmp_path):
    with pytest.raises(FileNotFoundError, match="download"):
        catalog(tmp_path)


def test_default_voice_needs_no_network_or_catalog(tmp_path):
    assert register_voice(None, "default", tmp_path) is None


def test_catalog_preserves_voice_metadata_for_selection(tmp_path):
    (tmp_path / "voices").mkdir()
    voice = {"name": "Zoey", "locale": "en-us", "language": "en"}
    (tmp_path / "voices" / "voices.jsonl").write_text(json.dumps(voice) + "\n")
    assert catalog(tmp_path) == [voice]
