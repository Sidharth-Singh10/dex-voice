"""Small, explicit user preferences. Invalid files are reported, never silently replaced."""

import json
import os
import sys
from dataclasses import asdict, dataclass, fields
from pathlib import Path


def data_directory() -> Path:
    if sys.platform == "win32":
        root = os.environ.get("LOCALAPPDATA")
        if not root:
            raise RuntimeError("LOCALAPPDATA is unavailable; cannot choose a data directory")
        return Path(root) / "DexVoice"
    return Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local" / "share")) / "dex-voice"


@dataclass
class Settings:
    models: str = ""
    tts_model: str = ""
    voice: str = "default"
    input_name: str = ""
    output_name: str = ""
    duration_delta: float = -0.3
    trim_tts: bool = False
    trim_threshold: float = 0.003

    def __post_init__(self):
        if not self.models:
            self.models = str(data_directory() / "models")

    @property
    def tts_directory(self) -> Path:
        return Path(self.tts_model) if self.tts_model else Path(self.models) / "phonon-31m-en"


def load_settings(path: Path) -> Settings:
    if not path.is_file():
        return Settings()
    try:
        values = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(values, dict):
            raise TypeError("root must be an object")
        types = {field.name: field.type for field in fields(Settings)}
        unknown = set(values) - set(types)
        if unknown:
            raise ValueError(f"unknown keys: {', '.join(sorted(unknown))}")
        settings = Settings(**values)
        for key, expected in types.items():
            if key not in values:
                continue
            value = values[key]
            if expected is bool and type(value) is not bool:
                raise TypeError(f"{key} must be a boolean")
            if expected is float and (type(value) is not float or value != value):
                raise TypeError(f"{key} must be a number")
            if expected is str and not isinstance(value, str):
                raise TypeError(f"{key} must be text")
        if not 0 < settings.trim_threshold < 1:
            raise ValueError("trim_threshold must be strictly between 0 and 1")
        if settings.duration_delta < -1 or settings.duration_delta > 1:
            raise ValueError("duration_delta is outside the supported range")
        return settings
    except (OSError, TypeError, ValueError, json.JSONDecodeError) as exc:
        raise ValueError(f"Could not read settings at {path}: {exc}") from exc


def save_settings(path: Path, settings: Settings) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(asdict(settings), indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)
