import time
from contextlib import contextmanager

import numpy as np
import pytest

from dex_voice.engine import EngineConfig, StreamingEngine
from dex_voice.streaming import WordAssembler


class FakeRecognizer:
    def __init__(self, words):
        self.words = WordAssembler()
        self.queue = list(words)

    def feed(self, block):
        return [self.queue.pop(0)] if self.queue else []


class FakeTts:
    sample_rate = 24000

    def __init__(self, *, fail=False, silent=False, audio_seconds=0.05):
        self.fail, self.silent, self.audio_seconds = fail, silent, audio_seconds
        self.texts = []

    @contextmanager
    def stream(self, text, *, voice=None):
        self.texts.append(text)
        if self.fail:
            raise RuntimeError("synthesis exploded")
        frames = int(self.sample_rate * self.audio_seconds)
        value = 0.0 if self.silent else 0.1
        yield iter([np.full(frames, value, np.float32)])


def wait_for(predicate, timeout=3):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(0.01)
    raise AssertionError("Condition was not reached before timeout")


def make(recognizer, tts, **config):
    engine = StreamingEngine(recognizer, tts, None, EngineConfig(**config))
    engine.start()
    return engine


def test_words_are_grouped_and_complete_words_are_spoken():
    recognizer = FakeRecognizer(["one", "two", "three", "four", "five"])
    tts = FakeTts()
    engine = make(recognizer, tts, group_words=2, group_ms=10000, trim_tts=False)
    try:
        engine.push_input(np.ones(320, np.float32))
        wait_for(lambda: engine.status()["committed"] >= 1)
        engine.push_input(np.ones(320, np.float32))
        wait_for(lambda: len(tts.texts) >= 1)
        engine.push_input(np.ones(320, np.float32))
        engine.push_input(np.ones(320, np.float32))
        wait_for(lambda: len(tts.texts) >= 2)
        assert tts.texts[:2] == ["one two", "three four"]
    finally:
        engine.stop()
    assert engine.error is None


def test_stop_flushes_a_partial_final_group_instead_of_losing_it():
    recognizer = FakeRecognizer(["alpha", "beta", "gamma"])
    tts = FakeTts()
    engine = make(recognizer, tts, group_words=3, group_ms=10000, trim_tts=False)
    engine.push_input(np.ones(320, np.float32))
    wait_for(lambda: engine.status()["committed"] >= 1)
    engine.stop()
    assert engine.error is None
    assert tts.texts[0] == "alpha"


def test_synthesis_failure_is_visible_and_stops_the_engine():
    recognizer = FakeRecognizer(["one", "two"])
    tts = FakeTts(fail=True)
    engine = make(recognizer, tts, group_words=1, trim_tts=False)
    engine.push_input(np.ones(320, np.float32))
    wait_for(lambda: engine.status()["error"] is not None)
    assert "Speech failed" in engine.status()["error"]
    engine.stop()
    assert engine.status()["running"] is False


def test_boundary_trim_cannot_turn_silent_synthesis_into_success():
    recognizer = FakeRecognizer(["one"])
    tts = FakeTts(silent=True)
    engine = make(recognizer, tts, group_words=1, trim_tts=True, trim_threshold=0.01)
    engine.push_input(np.ones(320, np.float32))
    wait_for(lambda: engine.status()["error"] is not None)
    assert "No speech" in engine.status()["error"]
    engine.stop()


def test_input_overflow_is_counted_not_blocked():
    recognizer = FakeRecognizer([])
    engine = StreamingEngine(recognizer, FakeTts(), None, EngineConfig(input_blocks=1))
    engine.push_input(np.ones(320, np.float32))
    engine.push_input(np.ones(320, np.float32))
    assert engine.status()["input_drops"] == 1


def test_output_overflow_is_visible_as_dropped_audio():
    engine = StreamingEngine(
        FakeRecognizer([]),
        FakeTts(),
        None,
        EngineConfig(output_buffer_seconds=0.02),
    )
    engine.output.push(np.ones(800, np.float32))
    engine.output.push(np.ones(800, np.float32))
    assert engine.status()["output_dropped_bytes"] > 0
    assert engine.status()["queued_seconds"] > 0


def test_latency_tracker_never_reports_zero_from_no_samples():
    engine = StreamingEngine(FakeRecognizer([]), FakeTts(), None, EngineConfig())
    summary = engine.status()["latency"]
    assert summary["count"] == 0 and summary["p50_ms"] is None


def test_engine_cannot_be_started_twice():
    engine = make(FakeRecognizer([]), FakeTts())
    try:
        with pytest.raises(RuntimeError, match="already"):
            engine.start()
    finally:
        engine.stop()
