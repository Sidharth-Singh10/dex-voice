import numpy as np
import onnx
import onnxruntime as ort
from onnx import TensorProto, helper

from dex_voice.prepare import prepare_fp16


def test_fp16_conversion_handles_fp32_only_range_without_original_value_info(tmp_path):
    constants = [
        helper.make_tensor(name, TensorProto.FLOAT, [], [value])
        for name, value in [("start", 0), ("limit", 2), ("delta", 1)]
    ]
    graph = helper.make_graph(
        [
            helper.make_node("Range", ["start", "limit", "delta"], ["r"]),
            helper.make_node("Cast", ["mask"], ["float_mask"], to=TensorProto.FLOAT),
            helper.make_node("Mul", ["x", "float_mask"], ["masked"]),
            helper.make_node("Mul", ["masked", "r"], ["y"]),
        ],
        "mixed_precision_regression",
        [helper.make_tensor_value_info("x", TensorProto.FLOAT, [2])],
        [helper.make_tensor_value_info("y", TensorProto.FLOAT, [2])],
        initializer=constants + [helper.make_tensor("mask", TensorProto.INT64, [2], [1, 1])],
    )
    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 18)])
    model.ir_version = 10
    onnx.save(model, str(tmp_path / "encoder.onnx"))
    converted = prepare_fp16(tmp_path)
    session = ort.InferenceSession(str(converted), providers=["CPUExecutionProvider"])
    result = session.run(None, {"x": np.array([2, 3], np.float32)})[0]
    np.testing.assert_array_equal(result, [0, 3])
