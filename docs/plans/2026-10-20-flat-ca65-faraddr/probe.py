"""Measure native far-address limits without guessing signedness."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent
FILES: dict[str, str] = {}


def cases() -> dict[str, str]:
    values = [
        "-16777217",
        "-16777216",
        "-8388609",
        "-8388608",
        "-1",
        "0",
        "8388607",
        "8388608",
        "16777215",
        "16777216",
        "4294967295",
        "$FFFFFFFF",
        "$1000000",
        "<$1000000",
        "16777215+1",
    ]
    out = {}
    for cpu in ["65816", "huc6280"]:
        preamble = f'.setcpu "{cpu}"\n.org $1000\n'
        for index, value in enumerate(values):
            line = f".faraddr {value}\n"
            for context, body in {
                "direct": line,
                "macro": ".macro emit\n" + line + ".endmacro\nemit\n",
                "unused": ".macro unused\n" + line + ".endmacro\n.byte 7\n",
                "included": line,
            }.items():
                name = f"{cpu}_{index}_{context}"
                if context == "included":
                    filename = name + ".inc"
                    FILES[filename] = body
                    body = f'.include "{filename}"\n'
                out[name] = preamble + body
        for name, body in {
            "forward": ".faraddr target\ntarget: .byte 7\n",
            "forward_high": ".faraddr target+$1000000\ntarget: .byte 7\n",
            "forward_negative": ".faraddr target-$1000000\ntarget: .byte 7\n",
            "mixed": ".faraddr $ffffff,-1,target\ntarget: .byte 7\n",
            "explicit_extraction": ".lobytes $1000000\n.hibytes $1000000\n.bankbytes $1000000\n",
            "unresolved": ".faraddr MISSING\n",
        }.items():
            out[f"{cpu}_{name}"] = preamble + body
    assert len(out) == 132
    return out


if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location(
        "capture", ROOT.parent / "2026-10-19-ca65-scope-binding/probe.py"
    )
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    prepared = cases()
    runner.FILES = FILES
    runner.cases = lambda: prepared
    runner.main()
