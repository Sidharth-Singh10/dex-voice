"""Experimental TTS boundary trimming, never an input-microphone speech gate."""

import numpy as np


class BoundarySilenceTrimmer:
    """Hold undecided quiet frames until more speech or EOS makes their role known.

    Leading/trailing quiet audio keeps a guard. Interior pauses survive unchanged.
    Speech streams immediately after detection; an entire request is not buffered.
    The threshold is deliberately low, but listening/fidelity validation is still required.
    """

    def __init__(self, *, sample_rate: int, threshold=0.0003, guard_ms=50):
        if sample_rate <= 0 or guard_ms < 0 or not 0 < threshold < 1:
            raise ValueError("Invalid boundary trimming configuration")
        self.frame = max(1, sample_rate // 100)
        self.guard = round(sample_rate * guard_ms / 1000)
        self.threshold = threshold
        self.input_samples = 0
        self.output_samples = 0
        self.started = False
        self.closed = False
        self.quiet = np.zeros(0, np.float32)
        self.remainder = np.zeros(0, np.float32)

    def _consume(self, frame):
        if np.sqrt(np.mean(frame**2)) < self.threshold:
            self.quiet = np.concatenate([self.quiet, frame])
            if not self.started:
                self.quiet = self.quiet[-self.guard :] if self.guard else self.quiet[:0]
            return []
        output = [self.quiet, frame] if len(self.quiet) else [frame]
        self.quiet = np.zeros(0, np.float32)
        self.started = True
        self.output_samples += sum(len(piece) for piece in output)
        return output

    def feed(self, pcm):
        if self.closed:
            raise ValueError("Cannot append audio after finish")
        pcm = np.asarray(pcm, dtype=np.float32)
        if pcm.ndim != 1 or not np.all(np.isfinite(pcm)):
            raise ValueError("Expected finite mono PCM")
        self.input_samples += len(pcm)
        samples = np.concatenate([self.remainder, pcm])
        end = len(samples) // self.frame * self.frame
        output = [
            piece
            for offset in range(0, end, self.frame)
            for piece in self._consume(samples[offset : offset + self.frame])
        ]
        self.remainder = samples[end:]
        return output

    def finish(self):
        if self.closed:
            return []
        self.closed = True
        output = self._consume(self.remainder) if len(self.remainder) else []
        self.remainder = np.zeros(0, np.float32)
        if self.started and len(self.quiet):
            tail = self.quiet[: self.guard]
            if len(tail):
                output.append(tail)
                self.output_samples += len(tail)
        self.quiet = np.zeros(0, np.float32)
        return output
