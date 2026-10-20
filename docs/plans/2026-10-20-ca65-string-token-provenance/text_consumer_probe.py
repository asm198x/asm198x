"""Retain computed quote values at textual ca65 consumer boundaries."""

import importlib.util
from pathlib import Path

FILES = {'inc"': ".byte 8\n", 'bin"': "X"}


def cases() -> dict[str, str]:
    return {
        "quoted_include": '.include .concat("inc",.sprintf("%c",34))\n.byte 7\n',
        "quoted_incbin": '.incbin .concat("bin",.sprintf("%c",34))\n.byte 7\n',
        "quoted_warning": '.warning .sprintf("%c",34)\n.byte 7\n',
        "quoted_error": '.error .sprintf("%c",34)\n',
        "quoted_assert": '.assert 0,error,.sprintf("%c",34)\n',
        "slash_warning": '.warning .sprintf("%c",92)\n.byte 7\n',
    }


if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location(
        "capture", Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
    )
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.FILES = FILES
    runner.main()
