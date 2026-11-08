# Python feasibility harness

This document preserves the original Python/ONNX feasibility harness instructions.
The native Rust application has its own runbook in [`../rust/README.md`](../rust/README.md)
and [`../rust/LINUX.md`](../rust/LINUX.md); the project overview is in
[`../README.md`](../README.md).

**Release target: Windows x64. Development and measurements: Linux or Windows.**

Local English microphone → **NVIDIA Parakeet Unified EN 0.6B** → **Gradium Phonon**
→ virtual microphone. Model and voice selection are separate. The required delay is
**strictly below 500 ms**, measured from an input word's end to the beginning of its
synthesized counterpart at the virtual microphone. Network/calling-app delay is excluded.

The repository contains the reproducible **feasibility harness** and an experimental
**Windows desktop application** (`DexVoice.exe`) that captures a microphone, runs the
models locally, and feeds a virtual cable. The GUI build is documented in
[WINDOWS.md](../WINDOWS.md).

**Measured result:** the current pipeline fails the deadline and accumulates playback
backlog. See [RESULTS.md](../RESULTS.md) for the native measurements and limitations. The
Windows executable passes its packaging self-test, but real CUDA/audio routing has not
yet been validated on Windows hardware.

## Setup

Use [uv](https://docs.astral.sh/uv/) and a current NVIDIA driver. The locked Python
environment includes ONNX Runtime GPU and its CUDA/cuDNN libraries; no PyTorch or NeMo
is needed for inference. The current lock uses CUDA 13, so the driver must support it.
Phonon's Windows/Linux x64 wheel requires AVX2/FMA/F16C (the i7-12650H qualifies).

```sh
uv sync --python 3.12 --extra export
uv run dex-voice download
uv run --extra export dex-voice prepare-fp16
```

Downloads are several GB and use pinned public Hugging Face revisions. Local FP16
conversion needs several GB of **system RAM**, not GPU RAM. Models, generated audio,
and reports are excluded from version control. The Python environment is local to
this directory; no system packages are changed.

## Run

Run commands from the project root. Supply a **new** output path for each experiment;
the harness refuses to overwrite previous results.

```sh
# CPU TTS in isolation: first PCM, synthesis duration, and resulting audio duration.
uv run dex-voice tts-benchmark --output runs/tts-default.json

# Recognition alone, with a real-time input clock and reference-aligned word delays.
uv run dex-voice benchmark --stt-only --output runs/stt-fp16

# Recognition and synthesis run concurrently; virtual playback includes queued duration.
uv run dex-voice benchmark --output runs/pipeline-default

# Repeat without resetting ASR between recordings, to expose accumulating delay.
uv run dex-voice benchmark --repeats 5 --output runs/pipeline-sustained

# Independent model and voice controls. A catalog voice is downloaded before timing.
uv run dex-voice voices
uv run dex-voice benchmark --voice Zoey --output runs/pipeline-zoey

# Compare tighter grouping and a faster Phonon speaking-rate condition.
uv run dex-voice benchmark --group-words 1 --group-ms 40 --duration-delta=-0.3 --output runs/pipeline-fast
```

### Experimental latency work

These switches are **off by default**. They do not establish sub-500 ms compliance:

```sh
# Remove generated boundary silence while preserving interior pauses and a 50 ms guard.
uv run dex-voice benchmark --voice Zoey --group-ms 240 --duration-delta=-0.3 --trim-tts --trim-threshold 0.003 --output runs/trim-experiment

# Truncate available encoder lookahead by one 80 ms frame; do not rewrite the checkpoint.
uv run dex-voice benchmark --stt-only --experimental-right-context 1 --output runs/context-experiment

# Combined sustained experiment (still fails the deadline).
uv run dex-voice benchmark --repeats 3 --voice Zoey --group-ms 240 --duration-delta=-0.3 --trim-tts --trim-threshold 0.003 --experimental-right-context 1 --output runs/combined-experiment
```

`--trim-threshold` is linear RMS energy, strictly between 0 and 1. The conservative
default is 0.0003; 0.003 removes more low-energy audio. The trimmer buffers undecided
quiet frames so that a later speech frame restores the **entire interior pause**.
It never uses future audio or backdates a retained chunk's availability. Requests
report both generated and retained durations. If no active audio is found, the run
fails rather than pretending a silent request was spoken. Quiet consonants and
overall output fidelity still require listening and independent output alignment.

`--experimental-right-context` counts encoder frames, not milliseconds or PCM samples.
It changes only the **available input window**, not the checkpoint's baked attention
mask. Therefore it is **not** a native NeMo 160 ms re-export. `1` retained this fixture's
transcript while reducing delay; `0` dropped a phrase and is a rejected diagnostic
configuration, not a usable faster mode. Warnings and effective context are recorded
in new reports. Original checkpoints and baseline reports remain untouched.

`--tts-model` accepts a local Phonon directory or an explicit `hf://owner/model@revision`.
Default: the downloaded English 31M model. Optional 40M models require acceptance of
their gated terms and are **CC BY-NC 4.0**, not commercially unrestricted. Phonon 31M
is Apache-2.0. The underlying NVIDIA model and voice catalog have separate terms;
check the upstream model cards before redistribution. No 40M weights are bundled.

## What the measurements mean

- `stt_word_commit_latency`: recognized, complete words aligned to independent input
  reference word ends. Punctuation/case are normalized; substitutions, insertions, and
  deletions are reported, not silently discarded. No recognized words cannot pass.
- `encoder_compute` / `stt_step_compute`: warm inference timings. Parakeet uses its
  actual buffered streaming graph with a fixed history window and persistent RNN-T
  decoder state. It is **not** repeated whole-utterance offline recognition.
- `optimistic_playback_latency_lower_bound`: first PCM playback time of each text group,
  including previously queued audio. Every word in the group is assigned that same
  optimistic time. Silence and later word positions can only make actual latency worse.
- `end_to_end_within_500ms`: **false** when even the optimistic output misses the deadline;
  otherwise **null (unverified)**. It is never set true from first-byte timing.
- GPU memory is sampled device-wide, including other programs. Sampling can miss peaks.
- `virtual-output.wav` is an inspectable simulation of the output timeline, not a
  measurement through an actual recording endpoint.

The benchmark exits with status **2** for a demonstrated end-to-end deadline miss;
status 0 means the diagnostic ran, **not** that the product passed acceptance.

### Limitations and next gate

The included LibriSpeech dev-clean clip is read speech with externally forced-aligned
word timings, not hand-verified conversational data. A simple energy timeout flushes
pending words after pauses; it never gates ASR input and is not a production VAD.
Large queues cause explicit errors rather than silently dropping words. Phonon's
regular API streams **output audio**, not arbitrarily appended text in one persistent
synthesis context. Small groups can sound discontinuous and speak more slowly than
the input, even if inference is fast. The harness deliberately exposes that backlog.

Windows acceptance still requires WASAPI capture/playback, correct resampling, VB-CABLE
(`CABLE Input` playback → `CABLE Output` recording), actual synthesized-word onset
measurement, and sustained tests alongside the calling app / intended game. No Linux
virtual-microphone backend, Android work, Windows installer, or custom driver is included.

## Checks

```sh
uv run pytest
uv run ruff check src tests
uv run ruff format --check src tests
uv build
```

Tests run without models, network, a microphone, or a GPU. The native benchmark is the
separate hardware integration check. Do not weaken the 500 ms threshold, omit bad words,
or call a simulated player a measured virtual microphone to obtain a passing result.

## Sources

- [Parakeet Unified model card](https://huggingface.co/nvidia/parakeet-unified-en-0.6b)
- [Upstream streaming export and smoke tests](https://github.com/k2-fsa/sherpa-onnx/tree/master/scripts/nemo/parakeet-unified-en-0.6b)
- [Phonon runtime](https://github.com/gradium-ai/xn-ptts)
- [ONNX Runtime CUDA provider](https://onnxruntime.ai/docs/execution-providers/CUDA-ExecutionProvider.html)
- [Reference word alignments](https://huggingface.co/datasets/gilkeyio/librispeech-alignments)
