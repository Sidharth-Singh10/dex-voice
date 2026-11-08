# Feasibility results

## Verdict: the tested pipeline does not meet the required <500 ms

Measured locally on Linux using the target hardware: Intel i7-12650H and RTX 3050
4 GB Laptop GPU. This is **not a Windows build or virtual-microphone acceptance test**.
No microphone audio was recorded. No system audio devices/settings were changed.

The experiment uses the pinned Parakeet Unified **240 ms buffered-streaming export**,
CPU Phonon 31M English, a real-time-paced 7.435-second LibriSpeech read-speech clip,
and independent, externally forced-aligned word-end references. Exact environment,
per-word records, settings, and transcripts are in each run's `report.json`.

## Native measurements

| Experiment | Median | p95 | Maximum |
|---|---:|---:|---:|
| CPU Phonon: first PCM, 9 warm short-text requests | 20.8 ms | 38.7 ms | 49.0 ms |
| FP32 Parakeet: completed-word commitment | 536.4 ms | 666.9 ms | 668.5 ms |
| FP16 Parakeet + default Phonon: word commitment | 516.5 ms | 645.0 ms | 655.7 ms |
| FP16 + Zoey, faster speech/grouped text: word commitment | 511.0 ms | 644.4 ms | 652.4 ms |
| Same grouped configuration, 3 consecutive clip replays | 518.0 ms | 625.5 ms | 655.6 ms |

FP16 encoder computation was about 30 ms per step; the same clip's FP32 encoder took
about 53 ms. FP16 and FP32 produced identical normalized word sequences on this clip.
This single-clip comparison does not establish general FP16 accuracy parity.

Sampled device-wide GPU memory peaked at **2084 MiB** with FP16 and **3318 MiB** with
FP32. These include other applications and can miss short peaks. Phonon was on CPU.

## Continuous synthesis failed due to accumulated audio duration

Fast inference does not make the generated speech short enough to fit the source's
timing. Independent tiny text requests repeatedly restart Phonon's speech context.
For example, the warm single-word request `Hello.` generated 1.28 seconds of PCM,
despite returning its first chunk quickly. No generated leading/trailing silence was
trimmed, no time compression was applied, and no words were dropped to hide backlog.

| Pipeline configuration | Median optimistic output delay | Worst optimistic output delay |
|---|---:|---:|
| Default voice, 80 ms text grouping deadline | 11271 ms | 21141 ms |
| Zoey, 240 ms grouping, `duration_delta=-0.3` | 8946 ms | 15776 ms |
| Same Zoey configuration, 3 consecutive replays | 24310 ms | 46505 ms |

These are **optimistic lower bounds**, not acoustic word onsets: every word in a text
group gets the playback time of that group's first PCM sample. A virtual player
accounts for previously queued audio. Actual word positions, generated silence,
WASAPI, and VB-CABLE can add delay. A failed lower bound demonstrates failure of this
implementation; a successful lower bound would not demonstrate success.

The first clip's transcript uses `anymore` where the reference says `any more`.
The literal word-error calculation counts that as a substitution plus a deletion
(8.7% WER on 23 reference words). That spelling variance is reported rather than
silently removing unmatched words from the result. Timing statistics cover the
21 exactly matched words. On the three-clip run, 67 words matched.

## Reproduction

After the setup/download/conversion commands in
[`docs/PYTHON-HARNESS.md`](docs/PYTHON-HARNESS.md), choose unused paths:

```sh
uv run dex-voice tts-benchmark --output runs/tts-reproduction.json
uv run dex-voice benchmark --stt-only --precision fp32 --output runs/stt-fp32-reproduction
uv run dex-voice benchmark --output runs/pipeline-default-reproduction
uv run dex-voice benchmark --voice Zoey --group-ms 240 --group-words 3 --duration-delta=-0.3 --output runs/pipeline-zoey-reproduction
uv run dex-voice benchmark --repeats 3 --voice Zoey --group-ms 240 --group-words 3 --duration-delta=-0.3 --output runs/pipeline-sustained-reproduction
```

Known deadline failures exit with status 2. Inspect `virtual-output.wav` alongside
`report.json`; files are retained under `runs/` locally, not bundled in the package.

## What remains unproven

- Sub-500 ms input-word-end to actual output-word-onset latency.
- Conversational speech, accents, noise, interruptions, and gaming contention.
- Windows WASAPI/VB-CABLE behavior and an installable Windows application.
- Other Parakeet streaming context settings or a persistent incremental-text Phonon
  synthesis path. The current experiment does not prove those alternatives impossible.
- Whether safe silence trimming, shorter right context, better word-boundary commitment,
  and duration control can close the gap while preserving words and acceptable speech.

Do not ship this as a compliant voice changer or loosen the threshold to call it one.
The measured processing path needs further investigation before Windows UI/installer work.

## Optimization pass: boundary silence and lookahead

All results below use the same hardware and input fixture as the baseline. The model
weights are unchanged. No generated speech is time-compressed, no raw microphone audio
is passed through, and no missing words are silently treated as a speed improvement.
Optional processing remains disabled by default.

### Measured attempts, including rejected ones

| Attempt | Before → after | Decision |
|---|---|---|
| Zoey: boundary-only trim, RMS 0.0003, 50 ms guard | Worst optimistic output delay 15776 → 8497 ms | Keep as an opt-in experiment; fidelity unverified |
| Raise trimming threshold to 0.003 | 8497 → 2722 ms | Keep as a configurable experiment; quiet speech needs validation |
| More negative speaking-rate condition, -0.3 → -0.6 | 2722 → 3968 ms | Reject as an optimization; more negative was not faster here |
| Default voice instead of Zoey, otherwise same trim/group settings | 2722 → 2923 ms | No win; voice choice remains explicit |
| Default voice, single-word rather than grouped requests | 2923 → 3875 ms | Reject as an optimization; restart overhead outweighs waiting less |
| Append commas to encourage continuation | Small isolated prompts were essentially unchanged or longer | Not implemented |
| Available right context: 2 → 1 encoder frames | Median STT commitment ~515 → 433 ms; worst 567 ms | Keep as an explicitly unvalidated window-truncation experiment |
| Available right context: 2 → 0 encoder frames | Median 334 ms, but drops "turning away her eyes" | Reject: content regression, not a passing latency result |

The input-window experiments **do not change the baked attention mask** and have not
been checked against a native NeMo re-export at the shorter context. The one-frame
variant retained the baseline transcript on the included clip; that is not a general
accuracy or model-parity claim. Zero-lookahead loses content even on this small test.

Trimming is causal and boundary-only: the first retained PCM is not made available
before it was actually produced/classified. Interior pauses remain intact, including
when a pause spans generator chunk boundaries. A 50 ms guard retains quiet samples
adjacent to detected activity. Synthetic unit tests verify those mechanics, **not**
whether every quiet phoneme in real speech survives the energy threshold.

### Combined sustained result

Command:

```sh
uv run dex-voice benchmark --voice Zoey --group-ms 240 --group-words 3 --duration-delta=-0.3 --trim-tts --trim-threshold 0.003 --experimental-right-context 1 --repeats 3 --output runs/pipeline-trimmed-right1-sustained
```

| Metric | Baseline sustained | Combined experimental sustained |
|---|---:|---:|
| Median completed-word commitment | 518 ms | 435 ms |
| p95 completed-word commitment | 625 ms | 542 ms |
| Worst completed-word commitment | 656 ms | 568 ms |
| Median optimistic output delay | 24310 ms | 3032 ms |
| Worst optimistic output delay | 46505 ms | 5307 ms |
| Matched reference words | 67 | 67 |
| Matched words whose STT commitment misses 500 ms | 39 | 8 |
| Matched words whose optimistic output misses 500 ms | 67 | 67 |

Full evidence: `runs/pipeline-trimmed-right1-sustained/report.json` and
`virtual-output.wav`. First-pass and per-configuration reports are also retained
under `runs/`. Faster recognition and less boundary silence are measurable gains,
but the remaining synthesis/playback timing still accumulates delay. **The strict
500 ms requirement remains unmet; there is now an experimental Windows build, but no
compliant one.**

A repeat validation with the same combined settings reproduced the result: median
STT commitment **436 ms**, worst STT commitment **568 ms**, and worst optimistic
output delay **5312 ms** (`runs/pipeline-trimmed-right1-validation/report.json`).
It again matched 67 words, with 8 STT deadline misses and 67 optimistic output
deadline misses. This confirms the gains are not just a single faster run.

Verification: 67 tests, including boundary/interior silence handling, chunk-size
invariance, quiet guards, explicit silent-output failure, context validation, invalid
threshold rejection, settings validation, and engine lifecycle/error propagation.
Inverting the interior-silence protection was caught by its focused regression test;
the mutation was restored before final checks.

## Windows executable

A portable Windows x64 build is produced under Wine with PyInstaller:

```text
dist/DexVoice/DexVoice.exe
```

It is a one-folder build; `DexVoice.exe` requires the adjacent `_internal` directory.
The folder is about 2.6 GB and bundles Python 3.13, ONNX Runtime GPU, CUDA 13/cuDNN
libraries, PortAudio, libsndfile, Tk, Phonon, and the Microsoft VC++ runtime DLLs.
Models are not bundled; they are downloaded into `%LOCALAPPDATA%\DexVoice\models`
(the app can also reuse an existing prepared model folder).

Self-test verification (Wine, both the build prefix and a fresh empty prefix):

- **14 checks, 0 failures**: imports, Tk 8.6, PortAudio, libsndfile WAV roundtrip,
  Phonon runtime, ONNX Runtime.
- `CUDAExecutionProvider` is reported as available. Actual CUDA kernels, WASAPI,
  VB-CABLE, and live-call behavior still require a real Windows machine.

A first Windows run failed before the window appeared:

```text
Failed to load Python DLL ...\_internal\python313.dll
LoadLibrary: The specified module could not be found.
```

The cause was a packaging defect: `python313.dll` needs `VCRUNTIME140.dll`, and
`onnxruntime.dll` needs `MSVCP140.dll`/`MSVCP140_1.dll`/`MSVCP140_ATOMIC_WAIT.dll`.
Those Microsoft runtime files were excluded during the Linux/Wine build. The fixed
build bundles them from the Microsoft VC++ 14.51 runtime, and the clean-prefix
self-test now starts without a system Python or VC++ installation. This does **not**
mean CUDA or audio routing has been validated on Windows.

The build is unsigned (SmartScreen warning is expected) and remains experimental.
See `WINDOWS.md` for installation, routing, troubleshooting, and the still-unmet
latency requirement.

## Rust/Vulkan spike (Linux)

A native Rust ASR spike now exists under `rust/`: `transcribe-cpp`/ggml with the
Parakeet Unified GGUF, explicitly selected on the RTX 3050 via Vulkan.

Measured results:

- F16: **60.7× real time offline**, **0% WER**, 2.27 GB peak VRAM.
- Streaming: **p50 ~510 ms**, p95 ~820 ms, **13–14 of 23 words at or over 500 ms**.
- Q8_0 is slower offline (1.21×) and no better on latency.
- Token rows are not populated during streaming, so token-level commits are
  unavailable through the public API.
- The process can SIGSEGV during Vulkan teardown after writing its report.

The spike **fails the <500 ms gate** while passing accuracy, speed, and VRAM.
Full data and the runtime/API analysis are in
[`spike-results/REPORT.md`](spike-results/REPORT.md); reproduction is in
[`rust/README.md`](rust/README.md).
