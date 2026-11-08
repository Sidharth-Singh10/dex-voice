# -*- mode: python ; coding: utf-8 -*-
"""Portable Windows build. Run from the repository root under Windows Python.

The build is intentionally one-folder: CUDA, cuDNN and PortAudio are large, and a
one-file executable would unpack them again on every launch.
"""

import sysconfig
from pathlib import Path

from PyInstaller.utils.hooks import collect_dynamic_libs

ROOT = Path(SPECPATH).parent
SITE = Path(sysconfig.get_paths()["purelib"])

binaries = []
datas = []

for folder in (
    SITE / "nvidia" / "cu13" / "bin" / "x86_64",
    SITE / "nvidia" / "cudnn" / "bin",
):
    if folder.is_dir():
        binaries += [(str(path), "cuda") for path in sorted(folder.glob("*.dll"))]

for package in ("onnxruntime", "ptts"):
    binaries += collect_dynamic_libs(package)

# The TensorRT provider is not requested and needs libraries we do not bundle.
binaries = [row for row in binaries if "onnxruntime_providers_tensorrt" not in row[0]]
binaries = [
    row for row in binaries
    if not row[0].endswith(("nvblas64_13.dll", "cufftw64_12.dll"))
]

portaudio = SITE / "_sounddevice_data" / "portaudio-binaries" / "libportaudio64bit.dll"
if portaudio.is_file():
    binaries.append((str(portaudio), "_sounddevice_data/portaudio-binaries"))

libsndfile = SITE / "_soundfile_data"
if libsndfile.is_dir():
    binaries += [(str(path), "_soundfile_data") for path in sorted(libsndfile.glob("*.dll"))]

for name in ("vcruntime140.dll", "vcruntime140_1.dll"):
    path = Path(sysconfig.get_paths()["stdlib"]).parent / name
    if path.is_file():
        binaries.append((str(path), "."))

# Microsoft's redistributable runtime DLLs. Populate with packaging/fetch_msvc_runtime.sh.
# Without these, python313.dll or onnxruntime.dll fail to load on a clean Windows install.
msvc = ROOT / "build" / "msvc"
if msvc.is_dir():
    binaries += [(str(path), ".") for path in sorted(msvc.glob("*.dll"))]

for document in ("README.md", "RESULTS.md", "WINDOWS.md"):
    if (ROOT / document).is_file():
        datas.append((str(ROOT / document), "."))

analysis = Analysis(
    [str(ROOT / "packaging" / "entry.py")],
    pathex=[str(ROOT / "src")],
    binaries=binaries,
    datas=datas,
    hiddenimports=[
        "dex_voice.asr",
        "dex_voice.benchmark",
        "dex_voice.cli",
        "dex_voice.desktop",
        "dex_voice.devices",
        "dex_voice.engine",
        "dex_voice.hardware",
        "dex_voice.models",
        "dex_voice.prepare",
        "dex_voice.settings",
        "dex_voice.smoke",
        "dex_voice.streaming",
        "dex_voice.tts",
        "kaldi_native_fbank",
        "onnx.shape_inference",
        "onnxruntime.capi._pybind_state",
        "onnxruntime.capi.onnxruntime_inference_collection",
        "onnxruntime.capi.onnxruntime_pybind11_state",
        "onnxruntime.transformers.float16",
        "onnxruntime.transformers.onnx_model",
        "ptts",
        "sounddevice",
        "soundfile",
        "tkinter",
        "tkinter.filedialog",
        "tkinter.font",
        "tkinter.messagebox",
        "tkinter.ttk",
    ],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[str(ROOT / "packaging" / "rthook_dlls.py")],
    excludes=["matplotlib", "scipy", "torch", "transformers", "pytest"],
    noarchive=False,
    optimize=0,
)

pyz = PYZ(analysis.pure)

exe = EXE(
    pyz,
    analysis.scripts,
    [],
    exclude_binaries=True,
    name="DexVoice",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
)

collection = COLLECT(
    exe,
    analysis.binaries,
    analysis.datas,
    strip=False,
    upx=False,
    upx_exclude=[],
    name="DexVoice",
)
