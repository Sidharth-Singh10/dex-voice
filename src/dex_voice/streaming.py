"""Deterministic streaming primitives; all times are monotonic seconds."""

from dataclasses import dataclass, field

import numpy as np


def feature_window(features, *, center: int, left: int, chunk: int, right: int):
    """Return history/current/lookahead; never pad unavailable future audio."""
    if center < 0 or left < 0 or chunk <= 0 or right < 0:
        raise ValueError("Invalid streaming context")
    end = center + chunk + right
    if end > len(features):
        raise ValueError("Insufficient right context")
    start = center - left
    window = features[max(0, start) : end]
    if start < 0:
        window = np.pad(window, ((-start, 0), (0, 0)))
    return window


@dataclass
class WordAssembler:
    """A new SentencePiece word boundary commits the previous complete word."""

    pending: str = ""
    last_token_at: float = 0.0

    def push(self, piece: str, at: float) -> list[str]:
        if not piece or (piece.startswith("<") and piece.endswith(">")):
            return []
        if piece == "▁":
            self.last_token_at = at
            return self.flush()
        text = piece.removeprefix("▁")
        if not any(char.isalnum() for char in text):
            if self.pending:
                self.pending += text
            return []
        committed = self.flush() if piece.startswith("▁") else []
        self.pending += text
        self.last_token_at = at
        return committed

    def flush(self) -> list[str]:
        pending, self.pending = self.pending, ""
        return [pending] if pending else []


@dataclass
class PlaybackClock:
    """An optimistic virtual player: includes queue duration, excludes OS/device latency."""

    buffer_seconds: float = 0.02
    end: float | None = None
    gaps: list[float] = field(default_factory=list)

    def enqueue(self, *, ready_at: float, duration: float) -> float:
        if duration < 0 or self.buffer_seconds < 0:
            raise ValueError("Audio durations must be non-negative")
        start = max(ready_at + self.buffer_seconds, self.end or 0.0)
        if self.end is not None and start > self.end:
            self.gaps.append(start - self.end)
        self.end = start + duration
        return start

    def backlog_at(self, now: float) -> float:
        return max(0.0, (self.end or now) - now)


def summarize(seconds: list[float], *, limit: float = 0.5) -> dict | None:
    """No samples is unknown, never a passing zero-latency result."""
    if not seconds:
        return None
    values = np.asarray(seconds, dtype=np.float64)
    if not np.all(np.isfinite(values)):
        raise ValueError("Metrics must be finite")
    return {
        "count": len(seconds),
        "p50_ms": float(np.percentile(values, 50) * 1000),
        "p95_ms": float(np.percentile(values, 95) * 1000),
        "max_ms": float(values.max() * 1000),
        "deadline_misses": int(np.count_nonzero(values >= limit)),
    }
