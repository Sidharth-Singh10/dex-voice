import numpy as np

from dex_voice.audio import BoundarySilenceTrimmer


def collect(trimmer, chunks):
    pieces = [piece for chunk in chunks for piece in trimmer.feed(chunk)]
    pieces.extend(trimmer.finish())
    return np.concatenate(pieces) if pieces else np.zeros(0, np.float32)


def test_only_boundary_silence_is_removed_and_guard_audio_is_preserved():
    pcm = np.concatenate([np.zeros(50), np.ones(20), np.zeros(60)]).astype(np.float32)
    trim = BoundarySilenceTrimmer(sample_rate=1000, guard_ms=20)
    result = collect(trim, [pcm])
    np.testing.assert_array_equal(result, np.concatenate([np.zeros(20), np.ones(20), np.zeros(20)]))
    assert trim.input_samples == 130
    assert trim.output_samples == 60


def test_interior_silence_is_never_removed():
    pcm = np.concatenate([np.ones(20), np.zeros(100), np.ones(20)]).astype(np.float32)
    result = collect(BoundarySilenceTrimmer(sample_rate=1000), [pcm])
    np.testing.assert_array_equal(result, pcm)


def test_output_streams_before_end_of_request():
    trim = BoundarySilenceTrimmer(sample_rate=1000, guard_ms=10)
    assert trim.feed(np.zeros(30, np.float32)) == []
    output = trim.feed(np.ones(10, np.float32))
    np.testing.assert_array_equal(np.concatenate(output), np.r_[np.zeros(10), np.ones(10)])


def test_chunk_boundaries_do_not_change_the_result_and_partial_frames_survive():
    pcm = np.r_[np.zeros(31), np.ones(27), np.zeros(48)].astype(np.float32)
    once = collect(BoundarySilenceTrimmer(sample_rate=1000, guard_ms=10), [pcm])
    split = collect(
        BoundarySilenceTrimmer(sample_rate=1000, guard_ms=10),
        [pcm[i : i + 7] for i in range(0, len(pcm), 7)],
    )
    np.testing.assert_array_equal(split, once)


def test_all_quiet_input_is_not_mistaken_for_synthesized_speech():
    trim = BoundarySilenceTrimmer(sample_rate=1000)
    assert len(collect(trim, [np.zeros(100, np.float32)])) == 0


def test_quiet_consonant_preroll_is_kept():
    pcm = np.r_[np.zeros(20), np.full(20, 0.0001), np.ones(20)].astype(np.float32)
    result = collect(BoundarySilenceTrimmer(sample_rate=1000, guard_ms=20), [pcm])
    np.testing.assert_array_equal(result, pcm[20:])
