"""Measure authored private-looking bytes in quoted and unquoted source."""

import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    marker = "\x01ca65-string:0\x01"
    return {
        f"{cpu}_{kind}": f'.setcpu "{cpu}"\n.byte .sprintf("%c",65)\n.byte {value}\n'
        for cpu in ["6502", "65816", "huc6280"]
        for kind, value in [("quoted", f'"{marker}"'), ("unquoted", marker)]
    }


if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location(
        "capture", Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
    )
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
