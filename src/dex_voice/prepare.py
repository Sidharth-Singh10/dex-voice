"""Local FP16 conversion; retain FP32 model inputs/outputs for feature and CPU graphs."""

from pathlib import Path
from tempfile import NamedTemporaryFile


def prepare_fp16(directory: Path) -> Path:
    import onnx
    from onnxruntime.transformers import float16
    from onnxruntime.transformers.onnx_model import OnnxModel

    destination = directory / "encoder.fp16.onnx"
    weights = directory / "encoder.fp16.weights"
    if destination.exists() or weights.exists():
        raise FileExistsError(f"Refusing to overwrite existing conversion: {destination}")
    print("Converting Parakeet encoder to FP16 (CPU/RAM only)...", flush=True)
    # Infer on disk BEFORE loading external weights. Without intermediate type information,
    # the converter misses casts around FP32-only mask/shape ops (Range/Min/Max).
    # Keep the temporary graph beside its external weights; close it first for Windows.
    with NamedTemporaryFile(dir=directory, suffix=".inferred.onnx", delete=False) as temporary:
        inferred = Path(temporary.name)
    try:
        onnx.shape_inference.infer_shapes_path(str(directory / "encoder.onnx"), str(inferred))
        model = onnx.load(str(inferred), load_external_data=True)
        converted = float16.convert_float_to_float16(
            model,
            keep_io_types=True,
            disable_shape_infer=True,
        )
    finally:
        inferred.unlink()
    # Runtime's converter appends cast nodes; ONNX requires producer-before-consumer order.
    OnnxModel(converted).topological_sort()
    onnx.save_model(
        converted,
        str(destination),
        save_as_external_data=True,
        all_tensors_to_one_file=True,
        location=weights.name,
        size_threshold=1024,
    )
    onnx.checker.check_model(str(destination))
    print(f"Saved {destination}", flush=True)
    return destination
