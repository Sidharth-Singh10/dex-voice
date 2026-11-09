# Dex Voice (Rust)

Linux-tested Rust desktop application: Parakeet Unified F16 GGUF through
`transcribe.cpp`/Vulkan, Phonon 31M CPU synthesis, `cpal` capture/playback,
and a Rune UI. The Windows Rust executable has **not** been built or tested yet;
the separate `../WINDOWS.md` refers to the earlier Python executable.

The <500 ms end-to-end target is not met. Latency tuning is deferred; keep a
physical microphone available. The 30-second bounded output buffer currently
prioritizes complete speech over low latency.

## Prerequisites

- Rust 1.85+ (edition 2024)
- CMake, Ninja, a C++17 compiler, `glslc`
- Vulkan loader and drivers
- Vulkan headers and SPIRV-Headers at build time
- `libcblas` and `libpulse` development libraries (Linux; `cpal` uses
  PulseAudio-on-PipeWire to enumerate individual virtual sources/sinks)

On distributions with the packages installed:

```sh
sudo apt install libvulkan-dev spirv-headers glslc
```

This machine has the loader and `glslc` but not the headers packages, so the
headers are vendored under `build/third_party/`:

```sh
git clone --depth 1 --branch v1.4.341 \
  https://github.com/KhronosGroup/Vulkan-Headers build/third_party/Vulkan-Headers
git clone --depth 1 https://github.com/KhronosGroup/SPIRV-Headers \
  build/third_party/SPIRV-Headers
cmake -S build/third_party/SPIRV-Headers -B build/third_party/SPIRV-Headers/build \
  -DCMAKE_INSTALL_PREFIX=build/third_party/spirv-prefix
cmake --install build/third_party/SPIRV-Headers/build
```

## Models

The application uses the **F16** Parakeet GGUF (not Q8_0). On Linux, install
pinned files plus two English voices (Zoey and Juniper) with
`./scripts/install-models.sh` from the `rust/` directory. It verifies SHA-256
before accepting any model or voice and writes to
`~/.local/share/dex-voice/models` by default; set `DEX_MODELS` to use a different
directory. On this development machine the models already live at
`/home/levi/dex/models`. For a clean Windows
install, use the hash-checked model installer from PowerShell:

```powershell
.\scripts\install-models.ps1
```

It downloads about 1.3 GB into `%LOCALAPPDATA%\DexVoice\models`. It does not
install an audio driver. A virtual cable (e.g. VB-CABLE) must be installed
separately and selected as the output device in Settings; **do not send this
app's output to system-default speakers**. The installer script is not yet
verified on Windows hardware.

For the standalone ASR spike, the optional Q8_0 model can be fetched with:

```sh
curl --fail --location --continue-at - \
  --output models/gguf/parakeet-unified-en-0.6b-Q8_0.gguf \
  https://huggingface.co/handy-computer/parakeet-unified-en-0.6b-gguf/resolve/843b1b175ace04f5a89d379c79e2518c0d065b25/parakeet-unified-en-0.6b-Q8_0.gguf
```

SHA-256: see `models/gguf/SHA256SUMS`.

## Build

```sh
cd rust
VULKAN_SDK=$PWD/../build/third_party/Vulkan-Headers \
CMAKE_PREFIX_PATH=$PWD/../build/third_party/spirv-prefix:$PWD/../build/third_party/Vulkan-Headers \
CFLAGS=-I$PWD/../build/third_party/SPIRV-Headers/include \
CXXFLAGS=-I$PWD/../build/third_party/SPIRV-Headers/include \
cargo build --release
```

The native library is compiled from source and linked statically into the
binary. The first build takes several minutes. This machine also uses
`rust/.cargo/config.toml` to link `libcblas`.

## Desktop app and self-test

```sh
cd rust
DEX_MODELS=/home/levi/dex/models ./target/release/dex-voice
./target/release/dex-voice --self-test ../spike-results/dex-self-test.json
```

Select a real microphone, a **virtual sink** as output, and a voice installed
in the selected Phonon checkpoint under Settings. Create the routing with
`./scripts/setup-virtual-mic.sh`: it makes sink `dex_voice` plus a remapped
microphone `dex_mic` ("DexMicrophone"). The remap is deliberate — Discord and
some browsers hide raw `.monitor` sources. In the calling app select
**DexMicrophone** as the microphone. This app never changes your system's
default speaker/microphone. Save settings before Start.
Settings lists devices in explicit pickers; duplicate display names are
numbered, and the stored selection uses a PulseAudio device ID, not the name.
**Refresh** re-enumerates devices without opening a stream. `cargo run --release
-p dex-audio --bin devices` prints IDs and names to troubleshoot routing.
Settings are stored in
`~/.local/share/dex-voice/settings.json` on Linux or
`%LOCALAPPDATA%\DexVoice\settings.json` on Windows. `--self-test` loads both
models and processes an embedded fixture without opening audio devices; it
requires a working Vulkan GPU and reports recognized text, audible PCM, and
queue drops in JSON. Missing model files are reported before starting.

p50/p95 measure the time from the first non-empty recognition hypothesis for a phrase (committed or tentative) to the first sample pushed into the output queue. They include the phrase-boundary wait and synthesis time and exclude playback/device and acoustic latency. A phrase that produces no PCM records no sample.

The Linux installer provides Default, Zoey and Juniper; other voices appear
after installing compatible embeddings into the selected checkpoint's
`voices/` or `embeddings/` directory. The Windows installer currently provides
only Default. Do not confuse the catalog's metadata with installed voice files.

## Run

```sh
VULKAN_SDK=$PWD/../build/third_party/Vulkan-Headers \
CMAKE_PREFIX_PATH=$PWD/../build/third_party/spirv-prefix:$PWD/../build/third_party/Vulkan-Headers \
cargo run --release -p dex-asr-spike -- \
  --model ../models/gguf/parakeet-unified-en-0.6b-Q8_0.gguf \
  --wav ../models/parakeet-240ms/test_wavs/0.wav \
  --reference ../fixtures/2086-149220-0033.json \
  --device nvidia \
  --backend vulkan \
  --mode both \
  --out ../spike-results/asr-baseline.json
```

The warm-up stream runs before measurement so Vulkan shader compilation is not
counted.

### Streaming configurations

| Intent | Flags |
|---|---|
| 240 ms (baseline) | `--stream-chunk-ms 80 --right-ms 160` |
| 160 ms | `--stream-chunk-ms 80 --right-ms 80` |
| 560 ms | `--stream-chunk-ms 160 --right-ms 400` |

Left context defaults to 5600 ms, the model's training setting.

## What the report contains

- Selected Vulkan device (must be the RTX 3050, not the Intel iGPU)
- Offline transcript, WER against the fixture, stage timings
- Streaming transcript, per-word commit events, p50/p95/max commit delay
- Wall-clock real-time factor
- Peak `nvidia-smi` memory during the run

A commit delay is measured from the reference word's end to the moment the
word appears in `stream.text().committed`. It is a recognition latency, not
end-to-end voice latency.

## Gate

- Same normalized words as the Python/ONNX run, no missing words.
- Median commit delay ≤ 450 ms at 240 ms; report 160 ms separately.
- Sustained RTF ≥ 2×.
- Peak VRAM ≤ 3.5 GB.

If the gate fails, stop and reassess the runtime before porting the app.

## Spike result (2026-11-08)

Measured on this laptop; full data in [`../spike-results/REPORT.md`](../spike-results/REPORT.md).

- **F16 Vulkan: 60.7× real time offline, 0% WER, 2.27 GB peak VRAM.**
- **Streaming: p50 ~510 ms, p95 ~820 ms, 13–14/23 words ≥500 ms.**
- Q8_0 is slower offline (1.21×) and no better on latency.
- Changing chunk size (80/160 vs 160/80), commit policy, or committed/full
  text does not move the median below ~510 ms.
- Token rows are not populated during streaming; a token-level assembler
  produces no words.

The gate **fails on latency** while passing accuracy, speed and VRAM. Do not
build the application UI around this engine while claiming <500 ms.
