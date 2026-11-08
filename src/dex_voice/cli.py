"""CLI entry point. Hardware dependencies are loaded only by commands needing them."""

import argparse
import json
from pathlib import Path


def positive_int(value: str) -> int:
    number = int(value)
    if number <= 0:
        raise argparse.ArgumentTypeError("must be a positive integer")
    return number


def quiet_threshold(value: str) -> float:
    number = float(value)
    if not 0 < number < 1:
        raise argparse.ArgumentTypeError("must be finite and strictly between 0 and 1")
    return number


def voice_arguments(parser):
    parser.add_argument("--tts-model", help="Local Phonon directory or explicit hf:// checkpoint")
    parser.add_argument("--voice", default="default", help="English catalog voice; see voices")
    parser.add_argument("--tts-threads", type=positive_int, default=4)
    parser.add_argument("--duration-delta", type=float, default=0.0, help="Negative speaks faster")


def main() -> int:
    parser = argparse.ArgumentParser(description="Parakeet Unified → Phonon feasibility tools")
    parser.add_argument("--models", type=Path, default=Path("models"))
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("download", help="Download pinned public checkpoints (several GB)")
    commands.add_parser("voices", help="List selectable English Phonon voices")
    commands.add_parser("prepare-fp16", help="Convert the streaming encoder locally to FP16")
    replay = commands.add_parser(
        "benchmark", help="Paced fixture replay, no microphone or speakers"
    )
    replay.add_argument("--fixture", type=Path, default=Path("fixtures/2086-149220-0033.json"))
    replay.add_argument("--output", type=Path, required=True, help="New report directory")
    replay.add_argument("--repeats", type=positive_int, default=1)
    replay.add_argument("--precision", choices=["fp16", "fp32"], default="fp16")
    replay.add_argument("--stt-only", action="store_true")
    replay.add_argument(
        "--trim-tts", action="store_true", help="Experimental TTS boundary silence trim"
    )
    replay.add_argument("--trim-threshold", type=quiet_threshold, default=0.0003)
    replay.add_argument(
        "--experimental-right-context",
        type=int,
        choices=[0, 1, 2],
        help="Truncate available right encoder frames; baked attention mask stays unchanged",
    )
    replay.add_argument("--group-ms", type=positive_int, default=80)
    replay.add_argument("--group-words", type=positive_int, default=3)
    voice_arguments(replay)
    tts = commands.add_parser(
        "tts-benchmark", help="Warm CPU Phonon first-PCM and duration benchmark"
    )
    tts.add_argument("--text", action="append", help="Repeat flag for multiple text inputs")
    tts.add_argument("--repeats", type=positive_int, default=3)
    tts.add_argument("--output", type=Path, required=True)
    voice_arguments(tts)
    args = parser.parse_args()
    if hasattr(args, "tts_model"):
        args.tts_model = args.tts_model or str(args.models / "phonon-31m-en")
    from dex_voice.models import catalog, download

    if args.command == "download":
        download(args.models)
    elif args.command == "voices":
        for voice in catalog(args.models):
            if voice["language"] == "en":
                print(f"{voice['name']:30} {voice['locale']:6} {voice['description']}")
    elif args.command == "prepare-fp16":
        from dex_voice.prepare import prepare_fp16

        prepare_fp16(args.models / "parakeet-240ms")
    elif args.command == "benchmark":
        from dex_voice.benchmark import run

        report = run(args)
        return 2 if report["end_to_end_within_500ms"] is False else 0
    elif args.command == "tts-benchmark":
        from dex_voice.tts import load_tts, measure_tts

        if args.output.exists():
            raise FileExistsError(f"Refusing to overwrite {args.output}")
        engine, voice = load_tts(args.tts_model, args.voice, args.models, args.tts_threads)
        report = measure_tts(
            engine,
            voice,
            texts=args.text or ["Hello.", "I can hear you.", "We should go now."],
            repeats=args.repeats,
            duration_delta=args.duration_delta,
        )
        report.update({"model": args.tts_model, "voice": args.voice, "threads": args.tts_threads})
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
