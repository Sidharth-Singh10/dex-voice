import pytest

from dex_voice.benchmark import build_report, load_fixture


def test_fixture_validates_audio_rate_and_reference_bounds(tmp_path):
    import json

    import numpy as np
    import soundfile as sf

    sf.write(tmp_path / "sample.wav", np.zeros(1600), 16000)
    manifest = tmp_path / "sample.json"
    manifest.write_text(
        json.dumps(
            {
                "audio": "sample.wav",
                "words": [{"word": "hello", "start": 0, "end": 0.2}],
            }
        )
    )
    with pytest.raises(ValueError, match="outside"):
        load_fixture(manifest, repeats=1)


def test_optimistic_virtual_playback_miss_is_failure_not_a_pass():
    report = build_report(
        [{"word": "hello", "end": 1.0}],
        [{"word": "hello", "committed_at": 1.1, "playback_lower_bound": 2.0}],
        steps=[0.01],
        encoder=[0.005],
    )
    assert report["end_to_end_within_500ms"] is False
    assert report["optimistic_playback_latency_lower_bound"]["max_ms"] == pytest.approx(1000)


def test_optimistic_virtual_playback_success_is_not_device_acceptance():
    report = build_report(
        [{"word": "hello", "end": 1.0}],
        [{"word": "hello", "committed_at": 1.1, "playback_lower_bound": 1.2}],
        steps=[0.01],
        encoder=[0.005],
    )
    assert report["end_to_end_within_500ms"] is None


def test_missing_synthesized_audio_cannot_be_reported_as_a_success():
    report = build_report(
        [{"word": "hello", "end": 1.0}],
        [{"word": "hello", "committed_at": 1.1}],
        steps=[0.01],
        encoder=[0.005],
    )
    assert report["synthesized_words"] == 0
    assert report["end_to_end_within_500ms"] is None
