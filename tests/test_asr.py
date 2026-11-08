import pytest

from dex_voice.asr import effective_right_context


def test_exported_context_is_preserved_by_default():
    assert (
        effective_right_context({"right_feature_frames": "16", "subsampling_factor": "8"}, None)
        == 16
    )


def test_experimental_override_uses_encoder_frames_not_samples_or_milliseconds():
    assert (
        effective_right_context({"right_feature_frames": "16", "subsampling_factor": "8"}, 1) == 8
    )


@pytest.mark.parametrize("override", [-1, 3])
def test_experimental_override_cannot_read_beyond_the_exported_window(override):
    with pytest.raises(ValueError, match="override"):
        effective_right_context({"right_feature_frames": "16", "subsampling_factor": "8"}, override)
