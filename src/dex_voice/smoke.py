"""Executable smoke test: verifies bundled imports and native libraries without recording."""

import json
import os
import platform
import sys
import tempfile
from importlib import import_module
from pathlib import Path


def _check(name, function):
    try:
        return {"name": name, "ok": True, "detail": function()}
    except Exception as exc:
        return {"name": name, "ok": False, "detail": f"{type(exc).__name__}: {exc}"}


def run_self_test(destination: Path | None = None):
    report = {
        "platform": platform.platform(),
        "python": sys.version,
        "frozen": bool(getattr(sys, "frozen", False)),
        "executable": sys.executable,
        "bundle": getattr(sys, "_MEIPASS", None),
        "checks": [],
        "warnings": [],
    }

    for module in [
        "numpy",
        "soundfile",
        "sounddevice",
        "ptts",
        "onnxruntime",
        "kaldi_native_fbank",
        "tkinter",
        "tkinter.ttk",
    ]:
        report["checks"].append(
            _check(f"import {module}", lambda m=module: import_module(m).__name__)
        )

    def providers():
        import onnxruntime as ort

        available = ort.get_available_providers()
        report["cuda_provider"] = "CUDAExecutionProvider" in available
        return available

    report["checks"].append(_check("onnxruntime providers", providers))

    def audio_roundtrip():
        import numpy as np
        import soundfile as sf

        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "probe.wav"
            sf.write(path, np.zeros(1600, np.float32), 16000)
            pcm, rate = sf.read(path, dtype="float32")
        if rate != 16000 or len(pcm) != 1600:
            raise RuntimeError("WAV roundtrip changed the data")
        return "16 kHz WAV roundtrip"

    report["checks"].append(_check("libsndfile", audio_roundtrip))

    def audio_devices():
        import sounddevice as sd

        devices = sd.query_devices()
        inputs = sum(row["max_input_channels"] > 0 for row in devices)
        outputs = sum(row["max_output_channels"] > 0 for row in devices)
        if not devices:
            raise RuntimeError("No PortAudio devices found")
        return f"{inputs} input / {outputs} output endpoints"

    report["checks"].append(_check("portaudio", audio_devices))

    def phonon():
        import ptts

        if not hasattr(ptts, "DEFAULT_MODEL") or not hasattr(ptts, "TTS"):
            raise RuntimeError("ptts extension loaded without its public API")
        return str(ptts.DEFAULT_MODEL)

    report["checks"].append(_check("phonon runtime", phonon))

    def phonon_synth():
        folder = os.environ.get("DEXVOICE_SMOKE_TTS")
        if not folder:
            return "skipped; set DEXVOICE_SMOKE_TTS to a Phonon checkpoint folder"
        import ptts

        engine = ptts.TTS(folder, lang="en", device="cpu")
        pcm = engine.synth("Hello.")
        if len(pcm) < 2400:
            raise RuntimeError(f"Unexpectedly short synthesis: {len(pcm)} samples")
        return f"{len(pcm) / engine.sample_rate:.2f}s at {engine.sample_rate} Hz"

    report["checks"].append(_check("phonon synthesis", phonon_synth))

    def tk():
        import tkinter

        root = tkinter.Tk()
        root.withdraw()
        version = root.tk.call("info", "patchlevel")
        root.destroy()
        return version

    report["checks"].append(_check("Tk window system", tk))

    if not report.get("cuda_provider"):
        report["warnings"].append(
            "CUDAExecutionProvider is unavailable. On real Windows hardware, install a current "
            "NVIDIA driver; this build is not usable for recognition without CUDA."
        )
    if report["frozen"] and not report["bundle"]:
        report["warnings"].append("Frozen process has no extraction directory")

    text = json.dumps(report, indent=2)
    if destination:
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(text + "\n", encoding="utf-8")
    ok = all(check["ok"] for check in report["checks"])
    return text, ok
