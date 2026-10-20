"""Measure whole-value data directive bounds against native ca65."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent


def cases() -> dict[str, str]:
    out = {}
    for cpu in ["65816", "huc6280"]:
        preamble = f'.setcpu "{cpu}"\n.org $1000\n'
        for directive, width in [
            ("byte", 1),
            ("word", 2),
            ("addr", 2),
            ("dbyt", 2),
            ("dword", 4),
            ("faraddr", 3),
        ]:
            bits = width * 8
            values = [
                str(-(1 << (bits - 1)) - 1),
                str(-(1 << (bits - 1))),
                "-1",
                "0",
                "1",
                str((1 << (bits - 1)) - 1),
                str((1 << bits) - 1),
                str(1 << bits),
                "$FFFFFFFF",
            ]
            for index, value in enumerate(values):
                body = f".{directive} {value}\n"
                out[f"{cpu}_{directive}_{index}_direct"] = preamble + body
                out[f"{cpu}_{directive}_{index}_macro"] = (
                    preamble + ".macro emit\n" + body + ".endmacro\nemit\n"
                )
            operand = "<*" if width == 1 else "*"
            out[f"{cpu}_{directive}_counter"] = (
                preamble + f".{directive} {operand},{operand}\n"
            )
            out[f"{cpu}_{directive}_forward"] = (
                preamble + f".{directive} target\ntarget: .byte 7\n"
            )
            out[f"{cpu}_{directive}_unused"] = (
                preamble + f".macro unused\n.{directive} MISSING\n.endmacro\n.byte 7\n"
            )
        out[f"{cpu}_extraction"] = (
            preamble + ".lobytes $1000000\n.hibytes $1000000\n.bankbytes $1000000\n"
        )
    assert len(out) == 254
    return out


if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location(
        "capture", ROOT.parent / "2026-10-19-ca65-scope-binding/probe.py"
    )
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.FILES = {}
    runner.cases = cases
    runner.main()
