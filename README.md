# Dex Voice

**A local, Linux-first speech bridge that re-synthesizes your microphone in a different voice before it reaches the call.**

Dex Voice captures a physical microphone, recognizes speech locally with Parakeet, synthesizes the recognized text with a Phonon voice on the CPU, and publishes the result as a PipeWire virtual microphone. Audio never leaves the machine, and there is no account, telemetry, or hosted service.

The signal path works end to end, but this is a research prototype, not a real-time conversational voice changer. Speech is handled phrase by phrase after a pause, and the bridge is half-duplex. Read the [limitations](#limitations) before trying it.

## What it does

- Captures a microphone you select explicitly, without changing system audio defaults.
- Recognizes English speech locally with Parakeet Unified F16 through `transcribe.cpp` on Vulkan.
- Synthesizes phrases with a Phonon English checkpoint and an installed voice embedding on the CPU.
- Writes the synthesized audio into a PipeWire null sink named `DexVoice` and exposes it through a remapped source named `DexMicrophone`.
- Runs a Rune desktop interface that shows bridge state, transcript, routing, and diagnostics.

## Signal path

```text
physical microphone
        |
        v
  cpal capture on PipeWire's PulseAudio host
        |
        v
Parakeet Unified F16 GGUF
  transcribe.cpp / ggml / Vulkan
        |
        v
  recognized, committed words
        |
        v
Phonon English checkpoint, CPU
  Default, Zoey, or Juniper voice
        |
        v
  PipeWire null sink: DexVoice
        |
        v
  remapped source: DexMicrophone
        |
        v
application using a PipeWire/PulseAudio capture source
```

Everything in that path is local. The virtual microphone is a deliberate remap: some applications hide plain `.monitor` sources, while a remapped source is presented as a normal capture device.

## Screenshots

| Idle | Speaking |
|---|---|
| ![Idle state](docs/screenshots/desktop-idle.png) | ![Speaking state](docs/screenshots/desktop-speaking.png) |

| Failed | Settings |
|---|---|
| ![Failed state](docs/screenshots/desktop-failed.png) | ![Settings window](docs/screenshots/desktop-settings.png) |

A separate static showcase lives in [`website/`](website/README.md); it is documentation and presentation only, not the application.

## Getting started

The Rust application is the current desktop implementation and is Linux-only. It needs:

- Rust with edition 2024 support
- CMake, Ninja, a C++17 compiler, and `glslc`
- The Vulkan loader, Vulkan drivers, and Vulkan/SPIRV headers at build time
- `libcblas` and `libpulse` development libraries
- An NVIDIA GPU selected explicitly through Vulkan

On distributions with the packages available:

```sh
sudo apt install libvulkan-dev spirv-headers glslc libcblas-dev libpulse-dev
```

Vulkan headers can also be vendored under `build/third_party/`; the exact clone and CMake commands are in [`rust/README.md`](rust/README.md#prerequisites). `build/` is ignored by git.

From `rust/`, install the pinned models and voices, create the virtual microphone, build, and run:

```sh
cd rust
./scripts/install-models.sh
./scripts/setup-virtual-mic.sh
cargo build --release -p dex-voice
./target/release/dex-voice
```

The installer verifies every file against a pinned SHA-256 and writes to `~/.local/share/dex-voice/models` by default. Set `DEX_MODELS` to use another directory.

In the application:

1. Open **Settings**.
2. Choose your physical microphone under **Microphone**.
3. Choose the **DexVoice** virtual sink under **Virtual microphone output**. Do not choose your speakers.
4. Choose **Default**, **Zoey**, or **Juniper** as the voice.
5. Save, press **Start**, and wait for **Listening** before speaking.

In the receiving application, select **DexMicrophone** as the microphone. If it does not appear, check the application's microphone permissions, reopen it, and reselect the device. The application never changes your system default speaker or microphone.

To remove the virtual devices later, unload their PipeWire modules as described in [`rust/LINUX.md`](rust/LINUX.md).

The device-free self-test loads both models and processes an embedded fixture without opening audio hardware. It still requires a working Vulkan GPU:

```sh
./target/release/dex-voice --self-test self-test.json
```

Full build flags, the streaming spike, and troubleshooting are in [`rust/README.md`](rust/README.md).

## Models and licensing

Nothing model-related is committed to this repository. The Linux installer downloads pinned revisions and checks their hashes:

- **Parakeet Unified English F16 GGUF** for recognition.
- **Phonon English checkpoint** for CPU synthesis.
- **Default, Zoey, and Juniper** voice embeddings from the pinned Phonon voice catalog.

The Windows PowerShell installer (`rust/scripts/install-models.ps1`) provisions the recognizer and default voice for a clean Windows install of the Rust app, which has not been built or tested on Windows. It does not install an audio driver.

Licensing follows the upstream projects. The Phonon English checkpoint is Apache-2.0; the larger gated Phonon variants are CC BY-NC 4.0 and are not bundled. Parakeet and the voice catalog have their own model-card terms. Review those terms before redistributing weights or generated audio. The Rust workspace manifest declares MIT; this snapshot does not include a root LICENSE file.

## Limitations

- **Research prototype, not a product.** There is no installer, package, hosted download, account, or support commitment.
- **Phrase-based, not word-by-word.** Dex Voice waits for a pause before sending a phrase, so it is not suitable for live back-and-forth conversation. Long uninterrupted speech is cut at a cap rather than streamed continuously.
- **Half-duplex.** The microphone is muted while Dex Voice plays, and speech that overlaps its own output is discarded.
- **No echo cancellation.** Use headphones. If another application plays DexMicrophone through speakers, the bridge can hear itself; the half-duplex guard reduces this but is not a substitute for echo cancellation.
- **Output can accumulate delay.** Synthesis can produce more audio than the source speech occupied, and the queue is bounded. Long speech can therefore fall further behind.
- **Linux-only Rust application.** There is no Windows Rust build and no tested Windows audio route. The earlier Windows Python build is a separate, older artifact whose CUDA and virtual-cable behavior was never validated on real Windows hardware.
- **Device recovery is unreliable.** A present virtual sink can currently trigger a false device failure (see [issue #1](https://github.com/Sidharth-Singh10/dex-voice/issues/1)). Keep a physical microphone available and expect to restart the bridge after an output-device change.
- **Shutdown cleanup is unverified.** A crash has been reported when deliberately closing the application. Stop the bridge before closing the window.
- **English only.** The installed recognizer and voices are English. Boundary trimming is experimental and can remove quiet consonants; disable it if speech sounds clipped.
- **No barge-in.** Speech already generated is not cancelled when you start talking again.
- **Settings are explicit.** Start refuses a blank output but cannot verify that the selected physical device is actually virtual. Selecting speakers sends synthesized speech to the room.

## Repository layout

```text
rust/                 Rust workspace: desktop app, ASR, TTS, audio, core
  apps/dex-voice/     Rune desktop application
  crates/             dex-asr, dex-tts, dex-audio, dex-core
  scripts/            model installer, virtual microphone setup
website/              Next.js showcase site (not the application)
src/dex_voice/        Original Python feasibility harness and desktop prototype
tests/                Python tests for the harness
docs/                 Harness notes and project screenshots
fixtures/             Reference transcript and word-alignment fixture
spike-results/        Native ASR spike reports (generated audio not committed)
packaging/            Python/Windows packaging files
RESULTS.md            Measured harness results, including failed attempts
WINDOWS.md            Earlier Windows Python build notes
```

Model weights, generated audio, build output, Python environments, Rust targets, and Node.js output are excluded from version control.

## Status

This is a snapshot of work in progress, not a release. The pipeline is useful as an experiment, but it does not meet the original conversational target and should not be presented as a compliant real-time voice changer. The measured outcomes and rejected attempts remain in [`RESULTS.md`](RESULTS.md) and [`spike-results/REPORT.md`](spike-results/REPORT.md); they are not repeated here as product claims.

The original Python/ONNX harness is kept for reproducibility, not as the current application. Its setup and measurement definitions are in [`docs/PYTHON-HARNESS.md`](docs/PYTHON-HARNESS.md), and the earlier Windows build is described in [`WINDOWS.md`](WINDOWS.md).

## Documentation

- [`rust/README.md`](rust/README.md) — Rust build, models, spike, and self-test
- [`rust/LINUX.md`](rust/LINUX.md) — Linux runbook and verified behavior
- [`website/README.md`](website/README.md) — showcase site
- [`RESULTS.md`](RESULTS.md) — measured Python harness results
- [`docs/PYTHON-HARNESS.md`](docs/PYTHON-HARNESS.md) — preserved Python harness guide
- [`WINDOWS.md`](WINDOWS.md) — earlier Windows Python application

## Third-party components

- Parakeet Unified by NVIDIA, via the upstream model card and GGUF conversion.
- `transcribe.cpp` and `ggml` for the native GGUF/Vulkan runtime.
- Phonon runtime and voices by Gradium.
- PipeWire, PulseAudio compatibility, and `cpal` for audio routing.
- Rune for the desktop interface.
- Next.js, React, GSAP, and Tailwind CSS for the showcase site.
- Cabinet Grotesk, self-hosted under the ITF Free Font License.

Each component keeps its own license and terms; consult the upstream projects before redistributing code, weights, voices, or audio.
