import pytest

from dex_voice.metrics import align_words, evaluate


def test_alignment_does_not_hide_missing_or_repeated_words():
    reference = [{"word": word, "end": i + 1} for i, word in enumerate(["a", "cat", "sat"])]
    observed = [{"word": word, "committed_at": i + 1.1} for i, word in enumerate(["a", "a", "sat"])]
    result = align_words(reference, observed)
    assert result["insertions"] + result["substitutions"] + result["deletions"] == 1
    assert len(result["matches"]) == 2


def test_alignment_normalizes_punctuation_and_case_but_not_words():
    result = align_words([{"word": "hello", "end": 1}], [{"word": "Hello,", "committed_at": 1.2}])
    assert len(result["matches"]) == 1
    assert result["word_error_rate"] == 0


def test_word_latency_uses_reference_end_not_asr_frame_timestamp():
    result = evaluate(
        [{"word": "hello", "end": 1.0}],
        [{"word": "hello", "committed_at": 1.6}],
    )
    assert result["stt_word_commit_latency"]["max_ms"] == pytest.approx(600)
    assert result["stt_within_500ms"] is False
    assert result["end_to_end_within_500ms"] is None


def test_no_recognized_words_cannot_pass_deadline():
    result = evaluate([{"word": "hello", "end": 1}], [])
    assert result["stt_within_500ms"] is False
    assert result["alignment"]["deletions"] == 1


def test_actual_output_timing_is_required_for_end_to_end_acceptance():
    result = evaluate(
        [{"word": "hello", "end": 1}],
        [{"word": "hello", "committed_at": 1.2, "audio_ready_at": 1.3}],
    )
    assert result["end_to_end_within_500ms"] is None
