"""Pinned, public model downloads. No model code is downloaded or executed."""

import json
from pathlib import Path

ASR_REPO = "csukuangfj2/sherpa-onnx-nemo-parakeet-unified-en-0.6b-streaming-240ms"
ASR_REVISION = "4d98de8335864baa13d85d7aaaa1a7115b2254d4"
TTS_REPO = "gradium/phonon-31m-en"
TTS_REVISION = "92741c5b07d085ff840e26fea8ce43110b87852f"
VOICE_REPO = "gradium/phonon-voices"
VOICE_REVISION = "0ee17205f254b5abc43b266c8a0f215565c352f7"


def download(root: Path) -> None:
    from huggingface_hub import snapshot_download

    specs = [
        (
            ASR_REPO,
            ASR_REVISION,
            "parakeet-240ms",
            [
                "encoder.onnx",
                "encoder.weights",
                "decoder.onnx",
                "joiner.onnx",
                "tokens.txt",
                "test_wavs/0.wav",
                "*.md",
            ],
        ),
        (
            TTS_REPO,
            TTS_REVISION,
            "phonon-31m-en",
            [
                "config.json",
                "tokenizer.json",
                "model.q8.gguf",
                "default-voice.safetensors",
                "README.md",
            ],
        ),
        (VOICE_REPO, VOICE_REVISION, "voices", ["voices.jsonl", "README.md"]),
    ]
    for repo, revision, folder, files in specs:
        destination = root / folder
        print(f"Downloading {repo}@{revision} → {destination}", flush=True)
        snapshot_download(
            repo_id=repo,
            revision=revision,
            local_dir=destination,
            allow_patterns=files,
            token=False,
            max_workers=4,
        )
        (destination / "source.json").write_text(
            json.dumps({"repo": repo, "revision": revision}, indent=2) + "\n",
            encoding="utf-8",
        )


def catalog(root: Path) -> list[dict]:
    path = root / "voices" / "voices.jsonl"
    if not path.is_file():
        raise FileNotFoundError("Download models first: dex-voice download")
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]


def register_voice(tts, name: str, root: Path) -> str | None:
    if name == "default":
        return None
    from huggingface_hub import hf_hub_download

    matches = [
        row
        for row in catalog(root)
        if row["language"] == "en"
        and name.casefold()
        in {
            row["name"].casefold(),
            row["voice_id"].casefold(),
        }
    ]
    if len(matches) != 1:
        raise ValueError(f"Unknown English voice: {name!r}. Run dex-voice voices.")
    voice = matches[0]
    path = hf_hub_download(
        VOICE_REPO,
        voice["filename"],
        revision=VOICE_REVISION,
        token=False,
    )
    tts.add_voice(voice["name"], path)
    return voice["name"]
