"""Real-time-paced file replay. No microphone capture and no sound-device playback."""

import json
import time
from importlib.metadata import version
from pathlib import Path

import numpy as np

from dex_voice.metrics import evaluate
from dex_voice.streaming import summarize


def load_fixture(path: Path, *, repeats: int):
    import soundfile as sf

    if not 1 <= repeats <= 100:
        raise ValueError("Repeats must be between 1 and 100")
    manifest = json.loads(path.read_text(encoding="utf-8"))
    pcm, rate = sf.read(path.parent / manifest["audio"], dtype="float32")
    if rate != 16000 or pcm.ndim != 1 or len(pcm) == 0 or not np.all(np.isfinite(pcm)):
        raise ValueError("Fixture must be finite, non-empty, 16 kHz mono audio")
    duration = len(pcm) / rate
    reference = manifest["words"]
    if not reference:
        raise ValueError("Fixture must contain word-level timing references")
    previous = 0.0
    for word in reference:
        if not previous <= word["start"] < word["end"] <= duration:
            raise ValueError("Reference words are unordered or outside the audio")
        previous = word["end"]
    expanded = [
        dict(word, start=word["start"] + repeat * duration, end=word["end"] + repeat * duration)
        for repeat in range(repeats)
        for word in reference
    ]
    return np.tile(pcm, repeats), expanded, manifest


def build_report(reference, observed, *, steps, encoder):
    report = evaluate(reference, observed)
    lower_bounds = [
        observed[j]["playback_lower_bound"] - reference[i]["end"]
        for i, j in report["alignment"]["matches"]
        if "playback_lower_bound" in observed[j]
    ]
    stats = summarize(lower_bounds)
    # Even best-case output misses the deadline: the real device cannot be faster.
    # Passing these optimistic bounds never proves actual word-level/device latency.
    if stats and stats["deadline_misses"]:
        report["end_to_end_within_500ms"] = False
    report.update(
        {
            "encoder_compute": summarize(encoder),
            "stt_step_compute": summarize(steps),
            "optimistic_playback_latency_lower_bound": stats,
            "synthesized_words": sum("playback_lower_bound" in word for word in observed),
            "limitations": [
                "Input word timings are forced-aligned, not human-verified.",
                "Playback is an optimistic virtual clock, not WASAPI/VB-CABLE.",
                "Every word in a group uses its first PCM sample as an optimistic lower bound.",
                "A passing lower bound does not establish sub-500ms word-level output latency.",
                "Read speech on Linux does not establish Windows conversational performance.",
            ],
        }
    )
    return report


def run(args) -> dict:
    import soundfile as sf

    from dex_voice.asr import ParakeetStream
    from dex_voice.hardware import GpuMonitor
    from dex_voice.tts import TtsWorker, load_tts

    pcm, reference, manifest = load_fixture(args.fixture, repeats=args.repeats)
    if args.output.exists():
        raise FileExistsError(f"Choose a new output directory; refusing overwrite: {args.output}")
    args.output.mkdir(parents=True)
    gpu = GpuMonitor()
    gpu.start()
    worker = None
    try:
        if args.experimental_right_context is not None:
            print(
                "EXPERIMENT: truncating input right context; baked attention mask is unchanged. "
                "This is not a validated NeMo context preset.",
                flush=True,
            )
        print("Loading and warming Parakeet CUDA...", flush=True)
        recognizer = ParakeetStream(
            args.models / "parakeet-240ms",
            precision=args.precision,
            experimental_right_context=args.experimental_right_context,
        )
        recognizer.warmup()
        if not args.stt_only:
            print("Loading and warming Phonon CPU...", flush=True)
            tts, voice = load_tts(str(args.tts_model), args.voice, args.models, args.tts_threads)
        observed, pending, capture_lags = [], [], []
        origin = time.perf_counter()
        if not args.stt_only:
            worker = TtsWorker(
                tts,
                voice,
                origin=origin,
                duration_delta=args.duration_delta,
                trim_boundaries=args.trim_tts,
                trim_threshold=args.trim_threshold,
            )
        # Tail zeros are delivered at real-time speed, never made available early.
        padded = np.pad(pcm, (0, 8000))
        last_submit, silence_at = 0.0, None
        for offset in range(0, len(padded), 320):
            block = padded[offset : offset + 320]
            deadline = origin + (offset + len(block)) / 16000
            time.sleep(max(0.0, deadline - time.perf_counter()))
            capture_lags.append(max(0.0, time.perf_counter() - deadline))
            words = recognizer.feed(block)
            now = time.perf_counter()
            # A benchmark-only energy timeout flushes a pending final word after a pause.
            # It does not gate recognition; production requires a validated VAD policy.
            if np.sqrt(np.mean(block**2)) < 0.003:
                silence_at = silence_at if silence_at is not None else now
            else:
                silence_at = None
            if (
                silence_at is not None
                and now - silence_at >= 0.16
                and now - recognizer.words.last_token_at >= 0.12
            ):
                words.extend(recognizer.words.flush())
            emitted = [{"word": word, "committed_at": now - origin} for word in words]
            observed.extend(emitted)
            pending.extend(emitted)
            if pending and (
                len(pending) >= args.group_words
                or now - origin - last_submit >= args.group_ms / 1000
            ):
                if worker:
                    worker.submit(pending)
                pending = []
                last_submit = now - origin
        final = [
            {"word": word, "committed_at": time.perf_counter() - origin}
            for word in recognizer.words.flush()
        ]
        observed.extend(final)
        pending.extend(final)
        if worker:
            if pending:
                worker.submit(pending)
            worker.finish()
        report = build_report(
            reference, observed, steps=recognizer.step_seconds, encoder=recognizer.encoder_seconds
        )
        if args.trim_tts:
            report["limitations"].append(
                "Experimental TTS boundary trimming: quiet phoneme preservation and output "
                "word fidelity have not been validated by listening or independent alignment."
            )
        if args.experimental_right_context is not None:
            report["limitations"].append(recognizer.metadata["experiment_warning"])
        report.update(
            {
                "fixture": manifest,
                "repeats": args.repeats,
                "precision": args.precision,
                "voice": args.voice,
                "tts_model": str(args.tts_model),
                "duration_delta": args.duration_delta,
                "trim_tts_boundaries": args.trim_tts,
                "trim_threshold": args.trim_threshold,
                "experimental_right_context": args.experimental_right_context,
                "tts_threads": args.tts_threads,
                "group_ms": args.group_ms,
                "group_words": args.group_words,
                "capture_scheduling_lag": summarize(capture_lags),
                "transcript": " ".join(row["word"] for row in observed),
                "words": observed,
                "asr_metadata": recognizer.metadata,
                "versions": {name: version(name) for name in ["onnxruntime-gpu", "ptts", "numpy"]},
            }
        )
        if worker:
            report["tts_requests"] = worker.requests
            report["virtual_playback_gaps"] = summarize(worker.player.gaps)
            report["virtual_playback_end_seconds"] = worker.player.end
            report["input_duration_seconds"] = len(pcm) / 16000
            end = int((worker.player.end or 0) * tts.sample_rate) + 1
            output = np.zeros(end, dtype=np.float32)
            for at, chunk in worker.chunks:
                start = round(at * tts.sample_rate)
                output[start : start + len(chunk)] = chunk[: len(output) - start]
            sf.write(args.output / "virtual-output.wav", output, tts.sample_rate)
    finally:
        hardware = gpu.finish()
    report["hardware"] = hardware
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(
        json.dumps(
            {
                key: value
                for key, value in report.items()
                if key
                in {
                    "stt_word_commit_latency",
                    "encoder_compute",
                    "stt_step_compute",
                    "optimistic_playback_latency_lower_bound",
                    "end_to_end_within_500ms",
                    "transcript",
                    "hardware",
                }
            },
            indent=2,
        )
    )
    print(f"Full report: {args.output / 'report.json'}", flush=True)
    return report
