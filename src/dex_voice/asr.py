"""Parakeet Unified's bounded-context, stateful RNN-T streaming ONNX path.

The encoder recomputes a fixed history window, not the entire recording. Decoder
state persists across steps. Feature/normalization settings follow sherpa-onnx's
upstream Parakeet Unified streaming export and smoke test.
"""

import time
from pathlib import Path

import numpy as np

from dex_voice.streaming import WordAssembler, feature_window


def effective_right_context(metadata: dict, override: int | None) -> int:
    """Experimental input-window truncation, NOT a re-exported attention-mask preset."""
    original = int(metadata["right_feature_frames"])
    factor = int(metadata["subsampling_factor"])
    if override is None:
        return original
    if override < 0 or override * factor > original:
        raise ValueError("Right-context override must not exceed the exported window")
    return override * factor


class ParakeetStream:
    sample_rate = 16000

    def __init__(
        self,
        directory: Path,
        *,
        precision="fp16",
        profile=False,
        experimental_right_context=None,
    ):
        import kaldi_native_fbank as knf
        import onnxruntime as ort

        # Frozen builds add the bundled CUDA directory in a runtime hook. The wheel's
        # own lookup is best-effort and must not block a portable launch.
        try:
            ort.preload_dlls(directory="")
        except Exception:
            pass
        if "CUDAExecutionProvider" not in ort.get_available_providers():
            raise RuntimeError("CUDA is unavailable. CPU fallback cannot satisfy this experiment.")

        options = ort.SessionOptions()
        options.intra_op_num_threads = 1
        options.inter_op_num_threads = 1
        options.log_severity_level = 3
        options.add_session_config_entry("session.intra_op.allow_spinning", "0")
        options.add_session_config_entry("session.inter_op.allow_spinning", "0")
        options.enable_profiling = profile
        encoder_file = "encoder.fp16.onnx" if precision == "fp16" else "encoder.onnx"
        path = directory / encoder_file
        if not path.is_file():
            raise FileNotFoundError(f"Missing {path}; run dex-voice prepare-fp16 first")
        self.encoder = ort.InferenceSession(
            str(path),
            options,
            providers=[
                (
                    "CUDAExecutionProvider",
                    {
                        "arena_extend_strategy": "kSameAsRequested",
                        "cudnn_conv_algo_search": "HEURISTIC",
                        "cudnn_conv_use_max_workspace": "0",
                    },
                ),
                "CPUExecutionProvider",
            ],
        )
        if self.encoder.get_providers()[0] != "CUDAExecutionProvider":
            raise RuntimeError("Parakeet did not load on CUDA; refusing silent CPU fallback")
        # Tiny recurrent label-loop graphs are intentionally on CPU, avoiding GPU round trips.
        options.enable_profiling = False
        self.decoder = ort.InferenceSession(
            str(directory / "decoder.onnx"),
            options,
            providers=["CPUExecutionProvider"],
        )
        self.joiner = ort.InferenceSession(
            str(directory / "joiner.onnx"),
            options,
            providers=["CPUExecutionProvider"],
        )
        meta = self.encoder.get_modelmeta().custom_metadata_map
        if meta.get("streaming_model_type") != "nemo_parakeet_unified_streaming":
            raise ValueError("This is not the Parakeet Unified streaming export")
        self.left = int(meta["left_feature_frames"])
        self.chunk = int(meta["chunk_feature_frames"])
        self.right = effective_right_context(meta, experimental_right_context)
        self.subsampling = int(meta["subsampling_factor"])
        self.left_encoder = int(meta["left_encoder_frames"])
        self.layers = int(meta["pred_rnn_layers"])
        self.hidden = int(meta["pred_hidden"])
        self.normalize = meta.get("normalize_type") == "per_feature"
        self.metadata = dict(meta, effective_right_feature_frames=self.right)
        if experimental_right_context is not None:
            self.metadata["experiment_warning"] = (
                "Truncated input window; original baked attention mask unchanged. "
                "Not validated against a native NeMo re-export at this context."
            )
        self.tokens = {}
        for line in (directory / "tokens.txt").read_text(encoding="utf-8").splitlines():
            token, index = line.rsplit(" ", 1)
            self.tokens[int(index)] = token
        self.blank = int(meta["vocab_size"])
        self.fbank_options = knf.FbankOptions()
        self.fbank_options.frame_opts.dither = 0
        self.fbank_options.frame_opts.remove_dc_offset = False
        self.fbank_options.frame_opts.window_type = "hann"
        self.fbank_options.mel_opts.low_freq = 0
        self.fbank_options.mel_opts.num_bins = int(meta["feat_dim"])
        self.fbank_options.mel_opts.is_librosa = True
        self.reset()

    def reset(self):
        import kaldi_native_fbank as knf

        self.fbank = knf.OnlineFbank(self.fbank_options)
        self.center = 0
        self.discarded_frames = 0
        self.words = WordAssembler()
        self.step_seconds = []
        self.encoder_seconds = []
        self.state0 = np.zeros((self.layers, 1, self.hidden), np.float32)
        self.state1 = self.state0.copy()
        self.decoder_output, self.next0, self.next1 = self._decode(self.blank)

    def _decode(self, token):
        inputs = self.decoder.get_inputs()
        outputs = self.decoder.run(
            None,
            dict(
                zip(
                    [node.name for node in inputs],
                    [
                        np.array([[token]], np.int32),
                        np.array([1], np.int32),
                        self.state0,
                        self.state1,
                    ],
                    strict=True,
                )
            ),
        )
        return outputs[0], outputs[2], outputs[3]

    def _encode(self, window):
        inputs = self.encoder.get_inputs()
        signal = np.ascontiguousarray(window.T[None], dtype=np.float32)
        started = time.perf_counter()
        output = self.encoder.run(
            None,
            {
                inputs[0].name: signal,
                inputs[1].name: np.array([signal.shape[-1]], np.int64),
            },
        )[0]
        self.encoder_seconds.append(time.perf_counter() - started)
        return output

    def warmup(self):
        window = np.zeros((self.left + self.chunk + self.right, 128), np.float32)
        for _ in range(3):
            self._encode(window)
        self.reset()

    def feed(self, pcm: np.ndarray) -> list[str]:
        self.fbank.accept_waveform(self.sample_rate, pcm)
        committed = []
        while self.fbank.num_frames_ready >= self.center + self.chunk + self.right:
            started = time.perf_counter()
            start = max(0, self.center - self.left)
            end = self.center + self.chunk + self.right
            features = np.stack([self.fbank.get_frame(i) for i in range(start, end)])
            window = feature_window(
                features,
                center=self.center - start,
                left=self.left,
                chunk=self.chunk,
                right=self.right,
            )
            if self.normalize:
                window = (window - window.mean(0)) / (window.std(0) + 1e-5)
            encoded = self._encode(window)
            for frame in range(
                self.left_encoder, self.left_encoder + self.chunk // self.subsampling
            ):
                for _ in range(10):
                    nodes = self.joiner.get_inputs()
                    logits = self.joiner.run(
                        None,
                        {
                            nodes[0].name: encoded[:, :, frame : frame + 1],
                            nodes[1].name: self.decoder_output,
                        },
                    )[0]
                    token = int(np.argmax(logits))
                    if token == self.blank:
                        break
                    self.state0, self.state1 = self.next0, self.next1
                    self.decoder_output, self.next0, self.next1 = self._decode(token)
                    committed.extend(self.words.push(self.tokens[token], time.perf_counter()))
            self.center += self.chunk
            discard = max(0, self.center - self.left) - self.discarded_frames
            if discard:
                self.fbank.pop(discard)
                self.discarded_frames += discard
            self.step_seconds.append(time.perf_counter() - started)
        return committed
