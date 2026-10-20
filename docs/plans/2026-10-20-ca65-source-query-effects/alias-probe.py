"""Measure the existing source alias classifier before connecting #795."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent


def cases() -> dict[str, str]:
    out = {}
    for name, expression in {
        "difference": "RIGHT-LEFT",
        "chain": "FIRST+1",
        "unary": "-LEFT+RIGHT",
        "multiply": "LEFT*0",
        "cancel": "LEFT-LEFT",
        "missing_cancel": "MISSING-MISSING",
        "forward_difference": "RIGHT-LEFT",
        "alias_to_label": "LEFT",
        "constant": "2",
    }.items():
        prefix = (
            "LEFT: .byte 0\nRIGHT: .byte 0\nFIRST=RIGHT-LEFT\n"
            if not name.startswith("forward")
            else ""
        )
        source = (
            prefix
            + "ALIAS="
            + expression
            + "\n.ifconst ALIAS\n.byte 7\n.else\n.byte 9\n.endif\n"
        )
        if name.startswith("forward"):
            source += "LEFT: .byte 0\nRIGHT: .byte 0\n"
        out[name] = source
    assert len(out) == 9
    return out


if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location(
        "capture", ROOT.parent / "2026-10-19-ca65-scope-binding/probe.py"
    )
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
