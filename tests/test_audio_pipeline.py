import numpy as np
import pytest

from dex_voice.audio_pipeline import PcmQueue, StatefulResampler


def test_resampling_is_continuous_across_chunk_boundaries():
    signal = np.sin(2 * np.pi * np.arange(48000) * 220 / 48000).astype(np.float32)
    once = StatefulResampler(48000, 16000).process(signal)
    split = StatefulResampler(48000, 16000)
    pieces = [split.process(signal[i : i + 317]) for i in range(0, len(signal), 317)]
    np.testing.assert_allclose(np.concatenate(pieces), once, atol=1e-6)


def test_resampling_handles_silence_and_short_calls_without_nan():
    resampler = StatefulResampler(24000, 16000)
    output = [
        resampler.process(np.zeros(1, np.float32)),
        resampler.process(np.zeros(2, np.float32)),
    ]
    combined = np.concatenate([piece for piece in output if len(piece)])
    assert np.all(np.isfinite(combined))


def test_pcm_queue_returns_silence_for_underrun_without_advancing_corrupt_data():
    queue = PcmQueue(4096, block_align=4)
    queue.push(np.array([0.25, 0.5], np.float32))
    first = queue.pop(1)
    np.testing.assert_allclose(first[:, 0], [0.25])
    second = queue.pop(2)
    np.testing.assert_allclose(second[:, 0], [0.5, 0.0])


def test_pcm_queue_counts_overflow_instead_of_growing_without_bound():
    queue = PcmQueue(8, block_align=4)
    queue.push(np.ones(2, np.float32))
    queue.push(np.ones(3, np.float32))
    assert queue.dropped_bytes == 12
    assert queue.available == 8
    queue.clear()
    assert queue.available == 0
    np.testing.assert_array_equal(queue.pop(1), np.zeros((1, 1), np.float32))


def test_rejects_invalid_sample_rates():
    with pytest.raises(ValueError, match="positive"):
        StatefulResampler(0, 16000)
