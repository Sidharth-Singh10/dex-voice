import numpy as np
import pytest

from dex_voice.streaming import PlaybackClock, WordAssembler, feature_window, summarize


def test_feature_window_zero_pads_initial_history_and_preserves_right_context():
    features = np.arange(12, dtype=np.float32).reshape(6, 2)
    actual = feature_window(features, center=0, left=4, chunk=2, right=2)
    np.testing.assert_array_equal(actual[:4], np.zeros((4, 2)))
    np.testing.assert_array_equal(actual[4:], features[:4])


def test_feature_window_slices_sliding_history():
    features = np.arange(20, dtype=np.float32).reshape(10, 2)
    actual = feature_window(features, center=5, left=2, chunk=2, right=1)
    np.testing.assert_array_equal(actual, features[3:8])


def test_incomplete_window_is_rejected_instead_of_looking_into_future():
    with pytest.raises(ValueError, match="right context"):
        feature_window(np.zeros((3, 2)), center=0, left=2, chunk=2, right=2)


def test_only_complete_words_are_committed_and_subwords_are_joined():
    words = WordAssembler()
    assert words.push("▁Hel", 0.1) == []
    assert words.push("lo", 0.2) == []
    assert words.push(",", 0.3) == []
    assert words.push("▁world", 0.4) == ["Hello,"]
    assert words.flush() == ["world"]
    assert words.flush() == []


def test_late_punctuation_never_becomes_a_spoken_word():
    words = WordAssembler()
    words.push("▁hello", 0.1)
    assert words.flush() == ["hello"]
    assert words.push("▁.", 0.3) == []
    assert words.flush() == []


def test_standalone_sentencepiece_space_commits_previous_word():
    words = WordAssembler()
    words.push("▁certain", 0.1)
    words.push("ly", 0.2)
    assert words.push("▁", 0.3) == ["certainly"]
    assert words.push("very", 0.4) == []
    assert words.flush() == ["very"]


def test_playback_accounts_for_previously_queued_audio_not_just_tts_first_byte():
    clock = PlaybackClock(buffer_seconds=0.02)
    assert clock.enqueue(ready_at=1.0, duration=0.8) == pytest.approx(1.02)
    assert clock.enqueue(ready_at=1.1, duration=0.2) == pytest.approx(1.82)
    assert clock.backlog_at(1.1) == pytest.approx(0.92)


def test_playback_exposes_underflow_gap():
    clock = PlaybackClock(buffer_seconds=0.02)
    clock.enqueue(ready_at=0.0, duration=0.1)
    assert clock.enqueue(ready_at=0.4, duration=0.1) == pytest.approx(0.42)
    assert clock.gaps == [pytest.approx(0.3)]


def test_empty_metrics_are_not_reported_as_zero_latency():
    assert summarize([]) is None


def test_metrics_include_tail_maximum_and_strict_deadline_violations():
    result = summarize([0.1, 0.2, 0.5, 0.8], limit=0.5)
    assert result["count"] == 4
    assert result["max_ms"] == pytest.approx(800)
    assert result["deadline_misses"] == 2
    assert result["p95_ms"] > result["p50_ms"]
