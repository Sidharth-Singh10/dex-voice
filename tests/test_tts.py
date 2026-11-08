import time
from contextlib import contextmanager

import numpy as np
import pytest

from dex_voice.tts import TtsWorker, load_tts, measure_tts


class FakeTts:
    sample_rate = 1000

    def set_conditions(self, conditions):
        self.conditions = conditions

    @contextmanager
    def stream(self, text, *, voice=None):
        yield iter([np.zeros(80, dtype=np.float32), np.zeros(80, dtype=np.float32)])


def test_tts_measurement_preserves_audio_duration_and_request_count():
    engine = FakeTts()
    result = measure_tts(engine, None, texts=["hello", "world"], repeats=2, duration_delta=-0.3)
    assert len(result["requests"]) == 4
    assert all(row["audio_seconds"] == pytest.approx(0.16) for row in result["requests"])
    assert result["first_pcm"]["count"] == 4
    assert engine.conditions == {"duration_delta": -0.3}


def test_worker_preserves_order_and_accounts_for_playback_queue():
    worker = TtsWorker(FakeTts(), None, origin=time.perf_counter(), duration_delta=0)
    first = [{"word": "hello", "committed_at": 0}]
    second = [{"word": "world", "committed_at": 0}]
    worker.submit(first)
    worker.submit(second)
    worker.finish()
    assert [row["text"] for row in worker.requests] == ["hello", "world"]
    assert second[0]["playback_lower_bound"] >= first[0]["playback_lower_bound"] + 0.16


def test_worker_propagates_synthesis_failure():
    class BrokenTts:
        def set_conditions(self, conditions):
            pass

        def stream(self, *args, **kwargs):
            raise ValueError("synthesis failed")

    worker = TtsWorker(BrokenTts(), None, origin=time.perf_counter(), duration_delta=0)
    worker.submit([{"word": "hello"}])
    with pytest.raises(RuntimeError, match="worker failed"):
        worker.finish()


def test_invalid_thread_count_fails_before_loading_or_downloading_model(tmp_path):
    with pytest.raises(ValueError, match="threads"):
        load_tts("model", "default", tmp_path, threads=0)


def test_optional_trim_records_raw_and_retained_duration_without_dropping_inner_pause():
    class PaddedTts(FakeTts):
        @contextmanager
        def stream(self, text, *, voice=None):
            yield iter(
                [np.r_[np.zeros(100), np.ones(20), np.zeros(100), np.ones(20), np.zeros(200)]]
            )

    worker = TtsWorker(
        PaddedTts(),
        None,
        origin=time.perf_counter(),
        duration_delta=0,
        trim_boundaries=True,
    )
    worker.submit([{"word": "hello"}])
    worker.finish()
    request = worker.requests[0]
    assert request["raw_audio_seconds"] == pytest.approx(0.44)
    assert request["output_audio_seconds"] == pytest.approx(0.24)


def test_optional_trimmer_cannot_hide_a_silent_synthesis_failure():
    worker = TtsWorker(
        FakeTts(),
        None,
        origin=time.perf_counter(),
        duration_delta=0,
        trim_boundaries=True,
    )
    worker.submit([{"word": "hello"}])
    with pytest.raises(RuntimeError, match="worker failed"):
        worker.finish()
