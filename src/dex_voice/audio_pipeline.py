"""Real-time PCM conversion and bounded playback. No device calls from this module."""

import threading

import numpy as np


class StatefulResampler:
    """Linear resampler that carries its fractional position and history across calls."""

    def __init__(self, source_rate: int, target_rate: int):
        if source_rate <= 0 or target_rate <= 0:
            raise ValueError("Sample rates must be positive")
        self.step = source_rate / target_rate
        self.position = 0.0
        self.history = np.zeros(1, np.float32)

    def process(self, pcm) -> np.ndarray:
        pcm = np.asarray(pcm, dtype=np.float32)
        if pcm.ndim != 1 or not np.all(np.isfinite(pcm)):
            raise ValueError("Expected finite mono PCM")
        if not len(pcm):
            return np.zeros(0, np.float32)
        samples = np.concatenate([self.history, pcm])
        end = len(samples) - 1
        available = np.arange(self.position, end, self.step, dtype=np.float64)
        if not len(available):
            self.history = samples[-1:]
            return np.zeros(0, np.float32)
        left = np.floor(available).astype(np.intp)
        fraction = (available - left).astype(np.float32)
        output = samples[left] * (1 - fraction) + samples[left + 1] * fraction
        consumed = int(available[-1])
        self.position = available[-1] + self.step - consumed
        self.history = samples[consumed:]
        return output.astype(np.float32)


class PcmQueue:
    """Thread-safe bounded PCM blocks; overflow is counted, never silently discarded."""

    def __init__(self, capacity_bytes: int, *, block_align: int):
        if capacity_bytes <= 0 or block_align <= 0:
            raise ValueError("Invalid PCM queue size")
        self.capacity_bytes = capacity_bytes
        self.block_align = block_align
        self.parts = []
        self.available = 0
        self.dropped_bytes = 0
        self.lock = threading.Lock()

    def push(self, pcm: np.ndarray) -> None:
        data = np.ascontiguousarray(pcm, dtype=np.float32).view(np.uint8)
        with self.lock:
            if self.available + len(data) > self.capacity_bytes:
                self.dropped_bytes += len(data)
                return
            self.parts.append(data.copy())
            self.available += len(data)

    def pop(self, frames: int) -> np.ndarray:
        wanted = frames * self.block_align
        with self.lock:
            if not self.parts:
                return np.zeros((frames, 1), np.float32)
            combined = np.concatenate(self.parts)
            taken = combined[:wanted]
            self.available -= len(taken)
            self.parts = [combined[wanted:]] if len(combined) > wanted else []
        output = np.zeros(wanted, np.uint8)
        output[: len(taken)] = taken
        return output.view(np.float32)[:frames, None]

    def clear(self):
        with self.lock:
            self.parts.clear()
            self.available = 0
