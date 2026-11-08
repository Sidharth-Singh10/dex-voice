import argparse

import pytest

from dex_voice.cli import quiet_threshold


@pytest.mark.parametrize("text", ["nan", "inf", "0", "-0.1", "1", "2"])
def test_invalid_trimming_threshold_is_rejected_before_loading_models(text):
    with pytest.raises(argparse.ArgumentTypeError):
        quiet_threshold(text)


def test_valid_quiet_threshold_is_preserved():
    assert quiet_threshold("0.003") == 0.003
