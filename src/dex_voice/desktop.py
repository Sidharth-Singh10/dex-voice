"""Windows desktop control panel. All blocking work runs off the Tk thread."""

import os
import queue
import sys
import threading
import tkinter as tk
from pathlib import Path
from tkinter import messagebox, ttk

from dex_voice.settings import Settings, data_directory, load_settings, save_settings

APP_TITLE = "Dex Voice — experimental"
WARNING = (
    "Experimental build. The measured pipeline does NOT yet meet the 500 ms target "
    "(worst simulated group delay was ~5.3 s). Do not rely on it for live calls, and "
    "always keep a working physical microphone available."
)


def models_ready(root: Path) -> tuple[bool, list[str]]:
    required = {
        "ASR encoder": root / "parakeet-240ms" / "encoder.onnx",
        "ASR tokens": root / "parakeet-240ms" / "tokens.txt",
        "Phonon model": root / "phonon-31m-en" / "model.q8.gguf",
        "Phonon voice": root / "phonon-31m-en" / "default-voice.safetensors",
        "Voice catalog": root / "voices" / "voices.jsonl",
    }
    return all(path.is_file() for path in required.values()), [
        f"{label}: {path}" for label, path in required.items() if not path.is_file()
    ]


def fp16_ready(root: Path) -> bool:
    return (root / "parakeet-240ms" / "encoder.fp16.onnx").is_file()


class VoiceApp(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title(APP_TITLE)
        self.geometry("900x780")
        self.minsize(760, 680)
        self.events = queue.Queue()
        self.settings_path = data_directory() / "settings.json"
        self.settings = self._load_settings()
        self.engine = None
        self.input = None
        self.output = None
        self.busy = False

        self.columnconfigure(0, weight=1)
        self.rowconfigure(6, weight=1)
        self._build()
        self.protocol("WM_DELETE_WINDOW", self._close)
        self.after(100, self._drain)
        self.after(250, self._tick)
        self.refresh_devices()
        self._load_voices()
        self._refresh_run_state()

    # -- construction --------------------------------------------------------------------
    def _build(self):
        heading = ttk.Frame(self, padding=(16, 14, 16, 6))
        heading.grid(row=0, column=0, sticky="ew")
        ttk.Label(heading, text="Dex Voice", font=("Segoe UI", 18, "bold")).pack(anchor="w")
        ttk.Label(
            heading,
            text="NVIDIA Parakeet Unified English → Gradium Phonon → Windows virtual microphone",
            font=("Segoe UI", 10),
        ).pack(anchor="w", pady=(2, 0))

        banner = tk.Label(
            self,
            text=WARNING,
            justify="left",
            wraplength=700,
            anchor="w",
            background="#FFF4E5",
            foreground="#5A3B00",
            padx=12,
            pady=8,
            font=("Segoe UI", 9),
        )
        banner.grid(row=1, column=0, sticky="ew", padx=16, pady=(0, 10))

        models = ttk.LabelFrame(self, text="Models", padding=12)
        models.grid(row=2, column=0, sticky="ew", padx=16, pady=4)
        models.columnconfigure(1, weight=1)
        ttk.Label(models, text="Model cache").grid(row=0, column=0, sticky="w")
        self.models_var = tk.StringVar(value=self.settings.models)
        ttk.Entry(models, textvariable=self.models_var, state="readonly").grid(
            row=0,
            column=1,
            sticky="ew",
            padx=8,
        )
        ttk.Button(models, text="Open folder", command=self._open_models).grid(row=0, column=2)
        rows = ttk.Frame(models)
        rows.grid(row=1, column=0, columnspan=3, sticky="ew", pady=(10, 0))
        self.parakeet_state = tk.StringVar()
        self.phonon_state = tk.StringVar()
        self.fp16_state = tk.StringVar()
        for index, (label, variable) in enumerate(
            [
                ("Parakeet Unified:", self.parakeet_state),
                ("Phonon 31M English:", self.phonon_state),
                ("FP16 encoder:", self.fp16_state),
            ]
        ):
            ttk.Label(rows, text=label).grid(row=index, column=0, sticky="w")
            ttk.Label(rows, textvariable=variable).grid(row=index, column=1, sticky="w", padx=8)
        buttons = ttk.Frame(models)
        buttons.grid(row=2, column=0, columnspan=3, sticky="ew", pady=(10, 0))
        ttk.Button(buttons, text="Download models", command=self._download).pack(side="left")
        ttk.Button(buttons, text="Prepare FP16 encoder", command=self._prepare_fp16).pack(
            side="left",
            padx=8,
        )
        self.model_progress = ttk.Progressbar(buttons, mode="indeterminate", length=150)
        self.model_progress.pack(side="right")
        self._refresh_model_state()

        voice = ttk.LabelFrame(self, text="Voice", padding=12)
        voice.grid(row=3, column=0, sticky="ew", padx=16, pady=4)
        voice.columnconfigure(1, weight=1)
        ttk.Label(voice, text="Phonon voice").grid(row=0, column=0, sticky="w")
        self.voice_var = tk.StringVar(value=self.settings.voice)
        self.voice_box = ttk.Combobox(
            voice,
            textvariable=self.voice_var,
            state="readonly",
            values=["default"],
        )
        self.voice_box.grid(row=0, column=1, sticky="ew", padx=8)
        ttk.Button(voice, text="Reload list", command=self._load_voices).grid(row=0, column=2)
        ttk.Label(voice, text="Speaking rate").grid(row=1, column=0, sticky="w", pady=(8, 0))
        self.rate_var = tk.DoubleVar(value=self.settings.duration_delta)
        scale = ttk.Scale(
            voice,
            from_=-0.8,
            to=0.8,
            variable=self.rate_var,
            command=lambda _: self._rate_text(),
        )
        scale.grid(row=1, column=1, sticky="ew", padx=8, pady=(8, 0))
        self.rate_label = ttk.Label(voice, text="")
        self.rate_label.grid(row=1, column=2, sticky="e", pady=(8, 0))
        self._rate_text()
        ttk.Label(
            voice,
            text="Negative values speak faster. This changes Phonon's duration condition.",
            font=("Segoe UI", 8),
        ).grid(row=2, column=0, columnspan=3, sticky="w", pady=(4, 0))

        audio = ttk.LabelFrame(self, text="Audio routing", padding=12)
        audio.grid(row=4, column=0, sticky="ew", padx=16, pady=4)
        audio.columnconfigure(1, weight=1)
        ttk.Label(audio, text="Microphone").grid(row=0, column=0, sticky="w")
        self.input_var = tk.StringVar(value=self.settings.input_name)
        self.input_box = ttk.Combobox(
            audio,
            textvariable=self.input_var,
            state="readonly",
            values=[],
        )
        self.input_box.grid(row=0, column=1, sticky="ew", padx=8)
        ttk.Label(audio, text="Virtual microphone output").grid(
            row=1, column=0, sticky="w", pady=(8, 0)
        )
        self.output_var = tk.StringVar(value=self.settings.output_name)
        self.output_box = ttk.Combobox(
            audio,
            textvariable=self.output_var,
            state="readonly",
            values=[],
        )
        self.output_box.grid(row=1, column=1, sticky="ew", padx=8, pady=(8, 0))
        ttk.Button(audio, text="Refresh devices", command=self.refresh_devices).grid(
            row=0,
            column=2,
            rowspan=2,
            sticky="ns",
        )
        ttk.Label(
            audio,
            text=(
                "Select CABLE Input here. In Discord, Zoom, WhatsApp or a game, choose "
                "CABLE Output as the microphone. This app never sends your real mic to the cable."
            ),
            font=("Segoe UI", 8),
            wraplength=640,
            justify="left",
        ).grid(row=2, column=0, columnspan=3, sticky="w", pady=(6, 0))

        processing = ttk.LabelFrame(self, text="Processing", padding=12)
        processing.grid(row=5, column=0, sticky="ew", padx=16, pady=4)
        ttk.Label(processing, text="Words per group").grid(row=0, column=0, sticky="w")
        self.group_words = tk.IntVar(value=3)
        ttk.Spinbox(processing, from_=1, to=8, width=5, textvariable=self.group_words).grid(
            row=0,
            column=1,
            sticky="w",
            padx=8,
        )
        ttk.Label(processing, text="Group deadline (ms)").grid(
            row=0, column=2, sticky="w", padx=(16, 0)
        )
        self.group_ms = tk.IntVar(value=240)
        ttk.Spinbox(
            processing, from_=40, to=1000, increment=20, width=6, textvariable=self.group_ms
        ).grid(
            row=0,
            column=3,
            sticky="w",
            padx=8,
        )
        self.trim_var = tk.BooleanVar(value=self.settings.trim_tts)
        ttk.Checkbutton(
            processing,
            text="Trim boundary silence (experimental)",
            variable=self.trim_var,
        ).grid(row=1, column=0, columnspan=2, sticky="w", pady=(10, 0))
        ttk.Label(processing, text="Quiet threshold").grid(
            row=1, column=2, sticky="e", padx=(16, 0), pady=(10, 0)
        )
        self.threshold = tk.DoubleVar(value=self.settings.trim_threshold)
        ttk.Entry(processing, textvariable=self.threshold, width=8).grid(
            row=1,
            column=3,
            sticky="w",
            padx=8,
            pady=(10, 0),
        )

        run = ttk.Frame(self, padding=(16, 8, 16, 4))
        run.grid(row=6, column=0, sticky="nsew")
        run.columnconfigure(1, weight=1)
        self.start_button = ttk.Button(run, text="Start", command=self._start)
        self.start_button.grid(row=0, column=0, sticky="w")
        self.stop_button = ttk.Button(run, text="Stop", command=self._stop, state="disabled")
        self.stop_button.grid(row=0, column=1, sticky="w", padx=8)
        self.state_var = tk.StringVar(value="Idle")
        ttk.Label(run, textvariable=self.state_var, font=("Segoe UI", 10, "bold")).grid(
            row=0,
            column=2,
            sticky="e",
        )

        meters = ttk.Frame(run)
        meters.grid(row=1, column=0, columnspan=3, sticky="ew", pady=(10, 0))
        meters.columnconfigure(1, weight=1)
        ttk.Label(meters, text="Input").grid(row=0, column=0, sticky="w")
        self.input_meter = ttk.Progressbar(meters, maximum=0.2, value=0)
        self.input_meter.grid(row=0, column=1, sticky="ew", padx=8)
        ttk.Label(meters, text="Output").grid(row=1, column=0, sticky="w", pady=(4, 0))
        self.output_meter = ttk.Progressbar(meters, maximum=0.2, value=0)
        self.output_meter.grid(row=1, column=1, sticky="ew", padx=8, pady=(4, 0))

        self.stats_var = tk.StringVar(
            value="Group delay p50 — · p95 — · queued 0.0 s · dropped 0.0 s"
        )
        ttk.Label(run, textvariable=self.stats_var).grid(
            row=2,
            column=0,
            columnspan=3,
            sticky="w",
            pady=(8, 0),
        )

        transcript = ttk.LabelFrame(run, text="Transcript", padding=8)
        transcript.grid(row=3, column=0, columnspan=3, sticky="nsew", pady=(8, 0))
        transcript.columnconfigure(0, weight=1)
        transcript.rowconfigure(0, weight=1)
        self.transcript = tk.Text(transcript, height=6, wrap="word", state="disabled")
        self.transcript.grid(row=0, column=0, sticky="nsew")
        scroll = ttk.Scrollbar(transcript, command=self.transcript.yview)
        scroll.grid(row=0, column=1, sticky="ns")
        self.transcript.configure(yscrollcommand=scroll.set)
        run.rowconfigure(3, weight=1)

        footer = ttk.Frame(run)
        footer.grid(row=4, column=0, columnspan=3, sticky="ew", pady=(6, 0))
        ttk.Label(
            footer,
            text="Leave a working physical microphone available as a fallback.",
            font=("Segoe UI", 8),
        ).pack(side="left")
        self.error_var = tk.StringVar()
        ttk.Label(footer, textvariable=self.error_var, foreground="#9B1C1C").pack(side="right")

    # -- state helpers -------------------------------------------------------------------
    def _load_settings(self) -> Settings:
        try:
            return load_settings(self.settings_path)
        except ValueError as exc:
            messagebox.showwarning(
                "Settings not loaded",
                f"{exc}\n\nDefaults were loaded. The existing file was not changed.",
            )
            return Settings()

    def _save(self):
        self.settings.models = self.models_var.get()
        self.settings.voice = self.voice_var.get()
        self.settings.input_name = self.input_var.get()
        self.settings.output_name = self.output_var.get()
        self.settings.duration_delta = round(self.rate_var.get(), 2)
        self.settings.trim_tts = bool(self.trim_var.get())
        try:
            self.settings.trim_threshold = float(self.threshold.get())
        except (tk.TclError, ValueError):
            self.settings.trim_threshold = 0.003
        save_settings(self.settings_path, self.settings)

    def _post(self, function):
        self.events.put(function)

    def _drain(self):
        while True:
            try:
                function = self.events.get_nowait()
            except queue.Empty:
                break
            function()
        self.after(100, self._drain)

    def _tick(self):
        if self.engine is not None:
            status = self.engine.status()
            self.input_meter["value"] = min(status["input_rms"], 0.2)
            self.output_meter["value"] = min(status["output_rms"], 0.2)
            latency = status["latency"]
            p50 = "—" if latency["p50_ms"] is None else f"{latency['p50_ms']:.0f} ms"
            p95 = "—" if latency["p95_ms"] is None else f"{latency['p95_ms']:.0f} ms"
            dropped = status["output_dropped_bytes"] / (4 * 48000)
            self.stats_var.set(
                f"Group delay p50 {p50} · p95 {p95} · queued {status['queued_seconds']:.1f} s "
                f"· dropped {dropped:.1f} s · STT drops {status['input_drops']}"
            )
            self._set_transcript(status["transcript"])
            if status["error"]:
                self.error_var.set(status["error"])
                self._stop()
            elif not status["running"] and not self.busy:
                self.state_var.set("Stopped")
        self.after(250, self._tick)

    def _set_transcript(self, text):
        if self.transcript.get("1.0", "end").strip() == text.strip():
            return
        self.transcript.configure(state="normal")
        self.transcript.delete("1.0", "end")
        self.transcript.insert("end", text)
        self.transcript.see("end")
        self.transcript.configure(state="disabled")

    def _rate_text(self):
        value = self.rate_var.get()
        label = "faster" if value < -0.05 else "slower" if value > 0.05 else "neutral"
        self.rate_label.configure(text=f"{value:+.2f} ({label})")

    def _refresh_model_state(self):
        root = Path(self.models_var.get())
        ready, _ = models_ready(root)
        self.parakeet_state.set("downloaded" if ready else "missing")
        self.phonon_state.set("downloaded" if ready else "missing")
        self.fp16_state.set("ready" if fp16_ready(root) else "not prepared")
        self._refresh_run_state()
        return ready

    def _refresh_run_state(self):
        if not hasattr(self, "start_button"):
            return
        root = Path(self.models_var.get())
        ready, _ = models_ready(root)
        can_start = (
            ready
            and fp16_ready(root)
            and bool(self.input_var.get())
            and bool(self.output_var.get())
            and not self.busy
            and self.engine is None
        )
        self.start_button.configure(state="normal" if can_start else "disabled")

    def _load_voices(self):
        from dex_voice.models import catalog

        try:
            rows = [row for row in catalog(Path(self.models_var.get())) if row["language"] == "en"]
        except (OSError, ValueError):
            self.voice_box["values"] = ["default"]
            return
        names = sorted({row["name"] for row in rows})
        self.voice_box["values"] = ["default", *names]
        if self.voice_var.get() not in self.voice_box["values"]:
            self.voice_var.set("default")
        self.voice_box.bind(
            "<<ComboboxSelected>>",
            lambda _: self._describe_voice(rows),
        )

    def _describe_voice(self, rows):
        selected = self.voice_var.get()
        for row in rows:
            if row["name"] == selected:
                self.state_var.set(f"{selected}: {row['description'][:110]}")
                return

    # -- device handling -----------------------------------------------------------------
    def refresh_devices(self):
        try:
            from dex_voice.devices import list_devices, query_devices

            devices, hostapis = query_devices()
            inputs = list_devices(devices, hostapis, kind="input")
            outputs = list_devices(devices, hostapis, kind="output")
        except Exception as exc:
            self.input_box["values"] = []
            self.output_box["values"] = []
            self.error_var.set(f"Audio devices unavailable: {exc}")
            return
        self.input_box["values"] = [device.label for device in inputs]
        self.output_box["values"] = [device.label for device in outputs]
        labels = {device.label for device in outputs}
        if self.output_var.get() not in labels:
            cable = next((label for label in labels if "CABLE Input" in label), "")
            self.output_var.set(cable)
        labels = {device.label for device in inputs}
        if self.input_var.get() not in labels and inputs:
            self.input_var.set(inputs[0].label)
        if not inputs or not outputs:
            self.error_var.set("No WASAPI microphone or output device found")
        self._refresh_run_state()

    def _open_models(self):
        path = Path(self.models_var.get())
        path.mkdir(parents=True, exist_ok=True)
        if sys.platform == "win32":
            os.startfile(path)
        else:
            messagebox.showinfo("Model folder", str(path))

    # -- model maintenance ---------------------------------------------------------------
    def _background(self, label, work):
        if self.busy:
            return
        self.busy = True
        self.start_button.configure(state="disabled")
        self.state_var.set(label)
        self.model_progress.start(12)

        def runner():
            try:
                message = work()
                self._post(lambda: self._finished(message))
            except Exception as exc:
                error = str(exc)
                self._post(lambda: self._failed(error))

        threading.Thread(target=runner, daemon=True).start()

    def _finished(self, message):
        self.busy = False
        self.model_progress.stop()
        self.state_var.set(message)
        self._refresh_model_state()
        self._load_voices()
        self._refresh_run_state()

    def _failed(self, exc):
        self.busy = False
        self.model_progress.stop()
        self.state_var.set("Failed")
        self.error_var.set(str(exc))
        self._refresh_run_state()
        messagebox.showerror("Dex Voice", str(exc))

    def _download(self):
        from dex_voice.models import download

        root = Path(self.models_var.get())

        def work():
            download(root)
            return "Models downloaded"

        self._background("Downloading models (several GB)…", work)

    def _prepare_fp16(self):
        from dex_voice.prepare import prepare_fp16

        root = Path(self.models_var.get())

        def work():
            if fp16_ready(root):
                return "FP16 encoder already prepared"
            prepare_fp16(root / "parakeet-240ms")
            return "FP16 encoder prepared"

        self._background("Preparing FP16 encoder (CPU/RAM, can take minutes)…", work)

    # -- run -----------------------------------------------------------------------------
    def _start(self):
        if self.engine is not None or self.busy:
            return
        root = Path(self.models_var.get())
        ready, missing = models_ready(root)
        if not ready:
            messagebox.showerror(
                None,
                "Download models first. Missing:\n\n" + "\n".join(missing),
            )
            return
        if not fp16_ready(root):
            messagebox.showerror(
                None,
                "Prepare the FP16 encoder first. It reduces GPU memory and latency; "
                "the app does not silently fall back to the slower FP32 graph.",
            )
            return
        try:
            threshold = float(self.threshold.get())
            if not 0 < threshold < 1:
                raise ValueError("Quiet threshold must be strictly between 0 and 1")
        except (tk.TclError, ValueError) as exc:
            messagebox.showerror(None, str(exc))
            return
        self._save()
        self.error_var.set("")
        self.busy = True
        self.start_button.configure(state="disabled")
        self.stop_button.configure(state="normal")
        self.state_var.set("Loading models and audio devices…")
        settings = self.settings
        config = {
            "group_words": int(self.group_words.get()),
            "group_ms": int(self.group_ms.get()),
            "trim_tts": bool(self.trim_var.get()),
            "trim_threshold": threshold,
        }
        threading.Thread(
            target=self._start_worker,
            args=(settings, config),
            daemon=True,
        ).start()

    def _start_worker(self, settings, config):
        try:
            from dex_voice.asr import ParakeetStream
            from dex_voice.devices import InputCapture, OutputPlayer, query_devices

            self._post(lambda: self.state_var.set("Loading Parakeet on CUDA…"))
            recognizer = ParakeetStream(
                Path(settings.models) / "parakeet-240ms",
                precision="fp16",
            )
            recognizer.warmup()
            self._post(lambda: self.state_var.set("Loading Phonon on CPU…"))
            from dex_voice.tts import load_tts

            tts, voice = load_tts(
                str(settings.tts_directory),
                settings.voice,
                Path(settings.models),
                4,
            )
            self._post(lambda: self.state_var.set("Opening audio devices…"))
            devices, hostapis = query_devices()
            from dex_voice.devices import resolve_device

            input_index = resolve_device(
                devices,
                hostapis,
                kind="input",
                name=settings.input_name,
            )
            output_index = resolve_device(
                devices,
                hostapis,
                kind="output",
                name=settings.output_name,
            )
            from dex_voice.engine import EngineConfig, StreamingEngine

            engine = StreamingEngine(recognizer, tts, voice, EngineConfig(**config))
            output = OutputPlayer(
                device=output_index,
                queue=engine.output,
                sample_rate=engine.cfg.output_rate,
                block_frames=480,
            )
            capture = InputCapture(
                device=input_index,
                block_frames=320,
                on_block=engine.push_input,
            )
            engine.start()
            output.start()
            capture.start()
            self._post(lambda: self._running(engine, capture, output))
        except Exception as exc:
            error = str(exc)
            self._post(lambda: self._failed_start(error))

    def _running(self, engine, capture, output):
        self.engine, self.input, self.output = engine, capture, output
        self.busy = False
        self.state_var.set("Running — choose CABLE Output in the calling app")
        self.stop_button.configure(state="normal")
        self._refresh_run_state()

    def _failed_start(self, exc):
        self.busy = False
        self.stop_button.configure(state="disabled")
        self.state_var.set("Failed to start")
        self.error_var.set(str(exc))
        self._refresh_run_state()
        messagebox.showerror("Could not start", str(exc))

    def _stop(self):
        if self.engine is None or self.busy:
            return
        self.busy = True
        self.state_var.set("Stopping…")
        self.stop_button.configure(state="disabled")
        capture, audio_out, engine = self.input, self.output, self.engine

        def work():
            try:
                if capture is not None:
                    capture.stop()
                engine.stop()
            finally:
                try:
                    if audio_out is not None:
                        audio_out.stop()
                finally:
                    self._post(self._stopped)

        threading.Thread(target=work, daemon=True).start()

    def _stopped(self):
        self.engine = self.input = self.output = None
        self.busy = False
        self.stop_button.configure(state="disabled")
        self.state_var.set("Idle")
        self.stats_var.set("Group delay p50 — · p95 — · queued 0.0 s · dropped 0.0 s")
        self._refresh_run_state()

    def _close(self):
        if self.engine is not None:
            try:
                if self.input is not None:
                    self.input.stop()
                self.engine.stop()
                if self.output is not None:
                    self.output.stop()
            except Exception:
                pass
        try:
            self._save()
        finally:
            self.destroy()


def main() -> int:
    if "--self-test" in sys.argv:
        from dex_voice.smoke import run_self_test

        index = sys.argv.index("--self-test")
        output = Path(sys.argv[index + 1]) if len(sys.argv) > index + 1 else None
        report, ok = run_self_test(output)
        if sys.stdout:
            print(report)
        return 0 if ok else 1
    os.environ.setdefault("HF_HOME", str(data_directory() / "huggingface"))
    os.environ.setdefault("HF_HUB_DISABLE_TELEMETRY", "1")
    data_directory().mkdir(parents=True, exist_ok=True)
    app = VoiceApp()
    app.mainloop()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
