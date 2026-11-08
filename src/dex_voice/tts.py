"""CPU Phonon synthesis with selected model/voice and diagnostic virtual playback."""

import os
import queue
import threading
import time
from pathlib import Path

import numpy as np

from dex_voice.audio import BoundarySilenceTrimmer
from dex_voice.models import register_voice
from dex_voice.streaming import PlaybackClock, summarize


def load_tts(model: str, voice: str, models: Path, threads: int):
    if not 1 <= threads <= 16:
        raise ValueError("Phonon CPU threads must be between 1 and 16")
    os.environ["RAYON_NUM_THREADS"] = str(threads)
    import ptts

    tts = ptts.TTS(model, lang="en", device="cpu")
    selected = register_voice(tts, voice, models)
    # Downloads/voice registration and kernel startup never count as warm inference.
    tts.synth("Ready.", voice=selected)
    return tts, selected


def measure_tts(tts, voice, *, texts: list[str], repeats: int, duration_delta: float):
    tts.set_conditions({"duration_delta": duration_delta})
    rows = []
    for _ in range(repeats):
        for text in texts:
            started = time.perf_counter()
            first, samples = None, 0
            with tts.stream(text, voice=voice) as stream:
                for pcm in stream:
                    if len(pcm):
                        first = first if first is not None else time.perf_counter() - started
                        samples += len(pcm)
            elapsed = time.perf_counter() - started
            rows.append(
                {
                    "text": text,
                    "first_pcm_seconds": first,
                    "compute_seconds": elapsed,
                    "audio_seconds": samples / tts.sample_rate,
                }
            )
    return {
        "first_pcm": summarize(
            [row["first_pcm_seconds"] for row in rows if row["first_pcm_seconds"] is not None]
        ),
        "requests": rows,
        "note": "First PCM may contain silence; it is not audible-word onset or device latency.",
    }


class TtsWorker:
    """A bounded text queue; overload is an explicit failure, never dropped words."""

    def __init__(
        self,
        tts,
        voice,
        *,
        origin: float,
        duration_delta: float,
        trim_boundaries=False,
        trim_threshold=0.0003,
    ):
        self.tts, self.voice = tts, voice
        self.origin, self.duration_delta = origin, duration_delta
        self.tts.set_conditions({"duration_delta": duration_delta})
        self.trim_boundaries = trim_boundaries
        self.trim_threshold = trim_threshold
        self.queue = queue.Queue(maxsize=128)
        self.player = PlaybackClock()
        self.chunks = []
        self.requests = []
        self.errors = []
        self.thread = threading.Thread(target=self._run, name="phonon-cpu", daemon=True)
        self.thread.start()

    def submit(self, words: list[dict]):
        if self.errors:
            raise RuntimeError("Phonon worker failed") from self.errors[0]
        self.queue.put_nowait(words)

    def _run(self):
        try:
            while (words := self.queue.get()) is not None:
                text = " ".join(word["word"] for word in words)
                started = time.perf_counter() - self.origin
                first = None
                stats = {"raw_samples": 0, "output_samples": 0}
                for pcm in self._audio(text, stats):
                    if not len(pcm):
                        continue
                    now = time.perf_counter() - self.origin
                    at = self.player.enqueue(ready_at=now, duration=len(pcm) / self.tts.sample_rate)
                    if at > 600:
                        raise RuntimeError("Virtual playback backlog exceeded the 10-minute limit")
                    self.chunks.append((at, np.asarray(pcm).copy()))
                    stats["output_samples"] += len(pcm)
                    if first is None:
                        first = now
                        for word in words:
                            word["playback_lower_bound"] = at
                self.requests.append(
                    {
                        "text": text,
                        "started_at": started,
                        "first_pcm_at": first,
                        "raw_audio_seconds": stats["raw_samples"] / self.tts.sample_rate,
                        "output_audio_seconds": stats["output_samples"] / self.tts.sample_rate,
                    }
                )
        except Exception as exc:
            self.errors.append(exc)

    def _audio(self, text, stats):
        trim = (
            BoundarySilenceTrimmer(
                sample_rate=self.tts.sample_rate,
                threshold=self.trim_threshold,
            )
            if self.trim_boundaries
            else None
        )
        with self.tts.stream(text, voice=self.voice) as stream:
            for pcm in stream:
                stats["raw_samples"] += len(pcm)
                yield from trim.feed(pcm) if trim else [pcm]
        if trim:
            yield from trim.finish()
            if not trim.started:
                raise RuntimeError("Experimental trimmer detected no active speech; result invalid")

    def finish(self):
        # Timed puts allow worker errors to propagate instead of deadlocking a full queue.
        while self.thread.is_alive():
            try:
                self.queue.put(None, timeout=0.1)
                break
            except queue.Full:
                continue
        self.thread.join(timeout=120)
        if self.thread.is_alive():
            raise TimeoutError("Phonon worker did not finish within 120 seconds")
        if self.errors:
            raise RuntimeError("Phonon worker failed") from self.errors[0]
