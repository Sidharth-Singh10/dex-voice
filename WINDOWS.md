# Windows application

**Status: experimental, and still outside the <500 ms requirement.** The measured
pipeline accumulates seconds of output delay in sustained speech. Do not present this
build as a compliant real-time voice changer. Keep a physical microphone available.

The build is a **portable folder**, not an installer. It contains `DexVoice.exe`, its
Python runtime, and the CUDA/cuDNN/PortAudio libraries it needs. Models are downloaded
separately so the build stays under control in size and licensing.

## Requirements

- Windows 10 or 11, x64.
- NVIDIA GPU and a current driver that supports **CUDA 13** (the bundled ONNX Runtime
  and NVIDIA libraries are CUDA 13 builds). The reference laptop is an RTX 3050 4 GB.
- [VB-CABLE](https://vb-audio.com/Cable/) installed by you. This build does **not**
  install or modify audio drivers.
- About 2.6 GB for the Parakeet ONNX export, ~80 MB for Phonon, and roughly 2 GB for
  the application folder.
- At least 8 GB of free disk space and, for the FP16 conversion, enough system RAM to
  load the 2.5 GB FP32 encoder (16 GB recommended).

## Routing

1. Install VB-CABLE and reboot if the installer asks.
2. Start `DexVoice.exe`.
3. **Microphone:** your real microphone.
4. **Virtual microphone output:** `CABLE Input (VB-Audio Virtual Cable)`.
5. In Discord/Zoom/WhatsApp/game, choose `CABLE Output (VB-Audio Virtual Cable)` as
   the microphone.
6. Leave your **headphones/speakers** as the Windows playback device. Choosing
   `CABLE Input` as the default playback device would also send the call audio into the
   cable and you would stop hearing other participants.

Requires the calling app to be configured to use a specific input device, or the user
to change the Windows default recording device.

## First run

1. **Download models.** This downloads pinned revisions of the Parakeet Unified
   streaming export, Phonon 31M English, and the voice catalog from Hugging Face.
2. **Prepare FP16 encoder.** Converts the 2.5 GB FP32 ONNX encoder to FP16 on CPU,
   writing a second file. This is the graph used by the app; without it the app refuses
   to start rather than silently running the slower FP32 graph.
3. **Reload list** under Voice to populate the English catalog voices.
4. Select devices and press **Start**. Wait for “Running”. Speak, then check the
   transcript and the group-delay figures.

The app saves settings to `%LOCALAPPDATA%\DexVoice\settings.json`; models live in
`%LOCALAPPDATA%\DexVoice\models` unless changed. Nothing in `Program Files` or the
Windows audio configuration is modified.

## Self-test

`DexVoice.exe --self-test %LOCALAPPDATA%\DexVoice\self-test.json` runs without opening
the window or touching microphone audio. It checks imports, Tk, PortAudio, libsndfile,
the Phonon extension, ONNX Runtime providers, and writes JSON. `cuda_provider: false`
means the machine cannot run recognition until the NVIDIA driver/CUDA runtime is
working. The check requires a desktop session for the Tk probe.

## Troubleshooting startup

**“Failed to load Python DLL … python313.dll”** means Windows could not load the bundled
interpreter or one of its dependencies. Check these in order:

1. Copy the **whole** `DexVoice` folder. `DexVoice.exe` needs the adjacent `_internal`
   folder, including `python313.dll`, `vcruntime140.dll`, `msvcp140.dll`, and `cuda\`.
2. The build now bundles the Microsoft VC++ runtime DLLs. An older build did not; if
   you have one, replace the whole folder with the current build.
3. Under **Windows Security → Virus & threat protection → Protection history**, check
   whether Defender quarantined a DLL. Restore it only if you trust the build.
4. Antivirus or a download manager that “optimizes” archives can drop files. Copying
   the folder directly over USB avoids that.
5. If it still fails, open PowerShell in the app folder and run:
   `Get-ChildItem _internal\python313.dll, _internal\vcruntime140.dll, _internal\msvcp140.dll`.
   All three must exist.

The Microsoft Visual C++ 2015–2022 x64 redistributable is normally installed by modern
games and applications; the portable build includes the runtime DLLs it needs, but
installing the redistributable is a safe fallback for unrelated native dependencies.

## What the numbers mean

The **group delay** shown in the app is an optimistic lower bound: end of a recognition
commit for a word group to the moment its first output PCM sample is queued for the
cable. It includes prior queued audio and generated speech, but **not** the cable's own
latency, the calling app's jitter buffer, or network delay. It is not an acoustic
end-to-end measurement.

`dropped` is output audio discarded because the two-second buffer filled. Any nonzero
value means the voice was interrupted. `STT drops` counts captured blocks lost before
recognition.

## Known limitations

- The <500 ms target is **not met**; see `RESULTS.md`.
- CUDA is required. There is deliberately no silent CPU fallback; CPU recognition was
  about three times slower than real time in upstream testing.
- English only. Phonon 40M models are not bundled and are CC BY-NC 4.0.
- Boundary trimming is experimental and can remove quiet consonants. Disable it if
  speech sounds clipped.
- No barge-in or cancellation of already generated speech; no echo cancellation.
- A missing, renamed, or disconnected device stops the run with an error; reselect and
  start again.
- The executable is unsigned, so SmartScreen may warn. No auto-update exists.

## Build from source

On Windows with Python 3.13 x64:

```powershell
py -3.13 -m pip install -r requirements-windows.txt
py -3.13 -m PyInstaller --clean --noconfirm packaging\windows.spec
```

`requirements-windows.txt` is generated from `pyproject.toml` with the `desktop`,
`export`, and `windows-build` extras. The output is `dist\DexVoice\`.

The Linux development environment can produce the same folder through Wine:

```sh
uv export --no-dev --extra desktop --extra export --group windows-build \
  --format requirements-txt --no-hashes --no-emit-project \
  --output-file build/windows-downloads/requirements.txt
```

Install a Windows CPython into a Wine prefix, `pip install` that file, then run
`wine C:\Python313\python.exe -m PyInstaller packaging/windows.spec`. Wine can verify
that the executable and native modules load; it **cannot** verify NVIDIA CUDA, WASAPI,
VB-CABLE, or a real call. Those remain Windows hardware checks.

## Rollback

The application is self-contained in its folder. To roll back, keep the previous
`DexVoice` folder and launch that instead. Settings and models are separate in
`%LOCALAPPDATA%\DexVoice`, so a code rollback does not redownload models. Deleting the
folder and that data directory removes the application completely.

## Privacy

Audio is processed locally by the models. Network access is used only to download
pinned model/voice files from Hugging Face. No telemetry or update check is included.
