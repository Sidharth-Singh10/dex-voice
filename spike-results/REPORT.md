# Rust ASR spike — Linux results

Date: 2026-11-08. Machine: i7-12650H, RTX 3050 4 GB Laptop, CachyOS,
NVIDIA 615.78.08, Vulkan 1.4.363, Rust 1.98.1.

Question: can the Rust stack replace the Python/ONNX runtime and meet the
latency gate?

**Answer: accuracy and speed pass; latency fails.** Median committed-word delay
is ~510 ms, with 12–14 of 23 words at or over 500 ms.

## Model and toolchain

| Item | Value |
|---|---|
| ASR runtime | `transcribe-cpp` 0.4.1, static ggml, Vulkan |
| Q8_0 model | `parakeet-unified-en-0.6b-Q8_0.gguf`, 731,357,568 bytes, SHA-256 `4b50b6dd…f38795` |
| F16 model | `parakeet-unified-en-0.6b-F16.gguf`, 1,239,114,240 bytes, SHA-256 `4a284b22…c35492` |
| Spike binary | 42 MB, includes ggml + Vulkan + Rust |
| Fixture | LibriSpeech `2086-149220-0033`, 7.435 s, 23 reference words |

Device selection was explicit: Vulkan enumerated the Intel iGPU first; the
spike selected `Vulkan1` (NVIDIA RTX 3050) and logged it.

## Results

`offRTF` is offline real-time factor. During streaming the input is paced at
real time, so wall time ≈ audio time; it only shows whether the engine keeps
up. Latency is measured from the reference word's end to its appearance in the
stream's committed/full text.

| Model | Config L/C/R ms | Source | Offline RTF | p50 | p95 | Max | ≥500 ms | WER | Peak VRAM |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Q8_0 | 5600/80/160 | text | 1.21× | 605 | 1653 | 2259 | 20/23 | 0% | 1640 MiB |
| Q8_0 | 5600/160/80 | text | — | 514 | 814 | 1309 | 12/23 | 0% | 1491 MiB |
| Q8_0 | 5600/160/80 | stable, agree=1 | — | 510 | 813 | 1312 | 12/23 | 0% | 1656 MiB |
| Q8_0 | 5600/560/0 | text | — | 687 | 1160 | 1313 | 17/23 | 4.35% | 1475 MiB |
| F16 | 5600/80/160 | text | 60.7× | 542 | 877 | 1201 | 15/23 | 0% | 2053 MiB |
| F16 | 5600/160/80 | text | 60.7× | 521 | 817 | 1309 | 14/23 | 0% | 1977 MiB |
| F16 | 5600/160/80 | full | — | 510 | 821 | 1309 | 13/23 | 0% | 2269 MiB |
| F16 | 5600/160/80 | tokens | — | — | — | — | — | 100% | 2113 MiB |

The token source produced no words: the public streaming snapshot does not
populate token rows during recognition, so token-level word commits are not
available through this API.

## What the numbers mean

- **Accuracy is excellent.** Q8_0 and F16 both match the reference exactly
  (0% WER). The ONNX conversion returned “anymore” against the reference’s
  “any more”; the GGUF returns “any more”.
- **F16 Vulkan is fast.** 7.435 s of audio encodes in 122 ms (60.7×). Compute
  is not the bottleneck.
- **Streaming keeps up.** Paced runs finish within ~60 ms of audio length.
- **The latency is in the commit policy.** Changing 80/160 → 160/80, commit
  policy Auto → Stable → agreement 1, or committed → full text moves the
  median only from ~605 to ~510 ms. The 560 ms config makes it worse and adds
  a spurious “Mm.”.
- **Token timestamps exist at finalize, not during streaming.** A token-based
  assembler cannot recover the earlier commits.

## Gate

| Criterion | Result |
|---|---|
| Same normalized words as reference | **Pass** (0% WER) |
| Median commit ≤ 450 ms | **Fail** (~510 ms) |
| Sustained RTF ≥ 2× | **Pass** (F16 60× offline; paced stream keeps up) |
| Peak VRAM ≤ 3.5 GB | **Pass** (max 2.27 GB) |
| Desktop integration | Not attempted; gate failed first |

## Comparison with the Python/ONNX path

| Path | p50 | p95 | ≥500 ms | Notes |
|---|---:|---:|---:|---|
| Python ONNX CUDA, 240 ms export | 516 ms | 645 ms | 12/21 | baseline |
| Python ONNX CUDA, right context −1 frame | 433 ms | 565 ms | 4/21 | experimental, baked mask unchanged |
| Rust GGUF Vulkan F16, 160/80 | 510–521 ms | 817 ms | 13–14/23 | 0% WER, single static binary |

The Rust switch is a packaging and accuracy win, not a latency win. The
only sub-450 ms measurement came from the experimental ONNX window truncation,
which was never validated against a native NeMo re-export.

## Teardown defect

After the JSON report is written, the process sometimes logs
`NVVM compilation failed: 3` and exits with SIGSEGV. Measurements are complete
before the crash. This is a Vulkan/ggml teardown issue (NVIDIA driver JIT), not
the recognition loop. A GUI build needs a clean shutdown; a `process::exit(0)`
after saving would mask it, but the cause should be found first.

## Options

1. **Continue Rust with transcribe.cpp F16 Vulkan.** 42 MB binary, 1.24 GB
   model, no CUDA DLLs, 0% WER, ~510 ms p50. Does **not** meet the strict
   <500 ms requirement.
2. **Port the ONNX token-level loop to Rust with `ort` + CUDA.** Closest to the
   measured 433 ms path, but preserves the 1.7 GB CUDA/ONNX runtime folder and
   the experimental window truncation.
3. **Add a token-level streaming API to transcribe.cpp.** The model has token
   timestamps; exposing tokens during streaming could reproduce the Python
   word assembler and target ~400–450 ms. Requires an upstream patch/fork.
4. **Stop the Rust port** and keep the Python/ONNX harness while latency is
   investigated.

Recommendation: do not build the Rune UI on option 1 while claiming the
500 ms requirement. Decide whether the product must be strictly under 500 ms;
if yes, option 2 or 3 is required.

## Reproduce

See `rust/README.md`. Reports are in `spike-results/`.

```sh
cd rust
./run-spike.sh --model ../models/gguf/parakeet-unified-en-0.6b-F16.gguf \
  --wav ../models/parakeet-240ms/test_wavs/0.wav \
  --reference ../fixtures/2086-149220-0033.json \
  --device nvidia --backend vulkan --mode both \
  --stream-chunk-ms 160 --right-ms 80 \
  --out ../spike-results/asr-f16.json
```
