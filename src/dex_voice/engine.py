"""Continuous capture → recognition → speech → bounded output, with explicit failure states."""

import queue
import threading
import time
from collections import deque
from dataclasses import dataclass

import numpy as np

from dex_voice.audio import BoundarySilenceTrimmer
from dex_voice.audio_pipeline import PcmQueue, StatefulResampler


@dataclass
class EngineConfig:
    group_words: int = 3
    group_ms: int = 240
    trim_tts: bool = True
    trim_threshold: float = 0.003
    input_blocks: int = 100
    output_buffer_seconds: float = 2.0
    silence_rms: float = 0.003
    silence_ms: int = 160
    output_rate: int = 48000


class LatencyTracker:
    def __init__(self, capacity=200):
        self.values = deque(maxlen=capacity)

    def add(self, seconds: float):
        self.values.append(seconds)

    def summary(self) -> dict:
        if not self.values:
            return {"count": 0, "p50_ms": None, "p95_ms": None, "max_ms": None}
        values = sorted(self.values)

        def pick(fraction):
            return values[min(len(values) - 1, round(fraction * (len(values) - 1)))]

        return {
            "count": len(values),
            "p50_ms": pick(0.5) * 1000,
            "p95_ms": pick(0.95) * 1000,
            "max_ms": values[-1] * 1000,
        }


class StreamingEngine:
    """Owns the real-time threads. Device I/O is injected through ``push_input``."""

    def __init__(self, recognizer, tts, voice, config: EngineConfig, *, clock=time.perf_counter):
        self.recognizer, self.tts, self.voice = recognizer, tts, voice
        self.cfg = config
        self.clock = clock
        self.input = queue.Queue(maxsize=config.input_blocks)
        self.tts_queue = queue.Queue(maxsize=128)
        self.resampler = StatefulResampler(tts.sample_rate, config.output_rate)
        self.output = PcmQueue(
            int(config.output_buffer_seconds * config.output_rate * 4),
            block_align=4,
        )
        self.latency = LatencyTracker()
        self.transcript = deque(maxlen=300)
        self.started_at = None
        self.stopping = threading.Event()
        self.threads = []
        self.error = None
        self.input_drops = 0
        self.committed = 0
        self.synthesized = 0
        self.last_input_rms = 0.0
        self.last_output_rms = 0.0
        self._lock = threading.Lock()
        self._pending = []

    # -- lifecycle -----------------------------------------------------------------------
    def start(self):
        if self.threads:
            raise RuntimeError("Engine is already started")
        self.started_at = self.clock()
        self.threads = [
            threading.Thread(target=self._recognize, name="dex-recognize", daemon=True),
            threading.Thread(target=self._synthesize, name="dex-phonon", daemon=True),
        ]
        for thread in self.threads:
            thread.start()

    def stop(self, timeout=30):
        if not self.threads:
            return
        self.stopping.set()
        deadline = time.monotonic() + timeout
        self._put_sentinel(self.input, self.threads[0], deadline)
        self.threads[0].join(max(0.0, deadline - time.monotonic()))
        if self.threads[0].is_alive():
            self.error = self.error or "Recognition did not stop within the timeout"
            return
        self._pending.extend(self.recognizer.words.flush())
        self._submit(self._pending)
        self._pending = []
        self._put_sentinel(self.tts_queue, self.threads[1], deadline)
        self.threads[1].join(max(0.0, deadline - time.monotonic()))
        if self.threads[1].is_alive():
            self.error = self.error or "Speech did not stop within the timeout"
        self.threads = []

    @staticmethod
    def _put_sentinel(target, thread, deadline):
        while thread.is_alive() and time.monotonic() < deadline:
            try:
                target.put(None, timeout=0.1)
                return
            except queue.Full:
                continue

    # -- capture side --------------------------------------------------------------------
    def push_input(self, pcm):
        pcm = np.asarray(pcm, dtype=np.float32)
        if pcm.ndim != 1 or not np.all(np.isfinite(pcm)):
            raise ValueError("Expected finite mono PCM")
        self.last_input_rms = float(np.sqrt(np.mean(pcm**2))) if len(pcm) else 0.0
        try:
            self.input.put_nowait(pcm)
        except queue.Full:
            self.input_drops += 1

    def _recognize(self):
        silence_since = None
        last_submit = 0.0
        try:
            while not (self.stopping.is_set() and self.input.empty()):
                try:
                    block = self.input.get(timeout=0.1)
                except queue.Empty:
                    continue
                if block is None:
                    break
                words = self.recognizer.feed(block)
                now = self.clock()
                if np.sqrt(np.mean(block**2)) < self.cfg.silence_rms:
                    silence_since = silence_since if silence_since is not None else now
                else:
                    silence_since = None
                if (
                    silence_since is not None
                    and now - silence_since >= self.cfg.silence_ms / 1000
                    and now - self.recognizer.words.last_token_at >= 0.12
                ):
                    words.extend(self.recognizer.words.flush())
                for word in words:
                    self._pending.append({"word": word, "committed_at": now - self.started_at})
                self.committed += len(words)
                self.transcript.extend(words)
                if self._pending and (
                    len(self._pending) >= self.cfg.group_words
                    or now - self.started_at - last_submit >= self.cfg.group_ms / 1000
                ):
                    self._submit(self._pending)
                    self._pending = []
                    last_submit = now - self.started_at
        except Exception as exc:
            self.error = f"Recognition failed: {exc}"
            self.stopping.set()

    def _submit(self, words):
        if not words:
            return
        try:
            self.tts_queue.put_nowait(list(words))
        except queue.Full:
            self.error = self.error or "Speech queue is full; synthesis cannot keep up"

    # -- synthesis side ------------------------------------------------------------------
    def _synthesize(self):
        try:
            while True:
                try:
                    words = self.tts_queue.get(timeout=0.2)
                except queue.Empty:
                    if self.stopping.is_set():
                        return
                    continue
                if words is None:
                    return
                self._speak(words)
        except Exception as exc:
            self.error = f"Speech failed: {exc}"
            self.stopping.set()

    def _speak(self, words):
        trimmer = (
            BoundarySilenceTrimmer(
                sample_rate=self.tts.sample_rate,
                threshold=self.cfg.trim_threshold,
            )
            if self.cfg.trim_tts
            else None
        )
        first_at = None
        text = " ".join(word["word"] for word in words)
        with self.tts.stream(text, voice=self.voice) as stream:
            for pcm in stream:
                chunks = trimmer.feed(pcm) if trimmer else [pcm]
                for chunk in chunks:
                    if self._enqueue(chunk, words, first_at) and first_at is None:
                        first_at = self.clock() - self.started_at
        if trimmer:
            for chunk in trimmer.finish():
                if self._enqueue(chunk, words, first_at) and first_at is None:
                    first_at = self.clock() - self.started_at
            if not trimmer.started:
                raise RuntimeError("No speech remained after boundary trimming")
        self.synthesized += len(words)

    def _enqueue(self, pcm, words, first_at):
        output = self.resampler.process(pcm)
        if not len(output):
            return False
        self.last_output_rms = float(np.sqrt(np.mean(output**2)))
        self.output.push(output)
        if first_at is None:
            at = self.clock() - self.started_at
            for word in words:
                word["playback_lower_bound"] = at
                self.latency.add(at - word["committed_at"])
        return True

    # -- diagnostics ---------------------------------------------------------------------
    def status(self) -> dict:
        with self._lock:
            return {
                "running": bool(self.threads) and not self.stopping.is_set(),
                "error": self.error,
                "input_rms": self.last_input_rms,
                "output_rms": self.last_output_rms,
                "input_drops": self.input_drops,
                "output_dropped_bytes": self.output.dropped_bytes,
                "queued_seconds": self.output.available / (4 * self.cfg.output_rate),
                "committed": self.committed,
                "synthesized": self.synthesized,
                "latency": self.latency.summary(),
                "transcript": " ".join(self.transcript),
            }
