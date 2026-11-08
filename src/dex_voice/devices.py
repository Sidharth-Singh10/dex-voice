"""Device discovery and PortAudio streams. Imported only by the desktop app."""

from dataclasses import dataclass

import numpy as np


@dataclass(frozen=True)
class Device:
    index: int
    label: str


def _wasapi_index(hostapis) -> int | None:
    for index, api in enumerate(hostapis):
        if "WASAPI" in api["name"]:
            return index
    return None


def list_devices(devices, hostapis, *, kind: str) -> list[Device]:
    """List capture/playback devices, preferring WASAPI and hiding duplicates."""
    if kind not in {"input", "output"}:
        raise ValueError("kind must be input or output")
    channel_key = "max_input_channels" if kind == "input" else "max_output_channels"
    wasapi = _wasapi_index(hostapis)
    preferred = [row for row in devices if row[channel_key] > 0 and row["hostapi"] == wasapi]
    others = [row for row in devices if row[channel_key] > 0 and row["hostapi"] != wasapi]
    seen, result = set(), []
    for row in preferred + others:
        name = row["name"].strip()
        if not name or name in seen:
            continue
        seen.add(name)
        suffix = "" if row["hostapi"] == wasapi else " (non-WASAPI)"
        result.append(Device(row["index"], f"{name}{suffix}"))
    return result


def resolve_device(devices, hostapis, *, kind: str, name: str) -> int:
    if not name:
        raise ValueError(f"Choose a {kind} device before starting")
    matches = [
        device for device in list_devices(devices, hostapis, kind=kind) if device.label == name
    ]
    if not matches:
        raise ValueError(f"The selected {kind} device is no longer available: {name}")
    return matches[0].index


def _sounddevice():
    import sounddevice as sd

    return sd


def query_devices():
    sd = _sounddevice()
    return sd.query_devices(), sd.query_hostapis()


class InputCapture:
    def __init__(self, *, device: int, sample_rate=16000, block_frames=320, on_block=None):
        sd = _sounddevice()
        self.on_block = on_block
        self.callback_errors = 0
        self.overflows = 0
        self.stream = sd.InputStream(
            device=device,
            channels=1,
            samplerate=sample_rate,
            blocksize=block_frames,
            dtype="float32",
            latency="low",
            callback=self._callback,
            extra_settings=sd.WasapiSettings(auto_convert=True),
        )

    def _callback(self, indata, frames, time_info, status):
        if status:
            self.overflows += 1
        try:
            self.on_block(np.asarray(indata[:, 0], dtype=np.float32).copy())
        except Exception:
            # Exceptions escaping a PortAudio callback can terminate the stream.
            self.callback_errors += 1

    def start(self):
        self.stream.start()

    def stop(self):
        self.stream.stop()
        self.stream.close()


class OutputPlayer:
    def __init__(self, *, device: int, queue, sample_rate=48000, block_frames=480):
        sd = _sounddevice()
        self.queue = queue
        self.callback_errors = 0
        self.stream = sd.OutputStream(
            device=device,
            channels=1,
            samplerate=sample_rate,
            blocksize=block_frames,
            dtype="float32",
            latency="low",
            callback=self._callback,
            extra_settings=sd.WasapiSettings(auto_convert=True),
        )

    def _callback(self, outdata, frames, time_info, status):
        try:
            outdata[:] = self.queue.pop(frames)
        except Exception:
            self.callback_errors += 1
            outdata.fill(0)

    def start(self):
        self.stream.start()

    def stop(self):
        self.stream.stop()
        self.stream.close()
