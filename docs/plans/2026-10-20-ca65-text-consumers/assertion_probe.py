"""Measure native assertion message boundaries and both assertion phases."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent


def cases() -> dict[str, str]:
    messages = {
        "absent": None,
        "missing": "",
        "empty": '""',
        "number": "42",
        "character": "'a'",
        "name": "MISSING",
        "plain": '"a"',
        "comma": '"a,b"',
        "adjacent": '"a""b"',
        "separated": '"a" "b"',
        "extra_value": '"a",42',
        "addition": '"a"+1',
        "computed_quote": '.sprintf("%c",34)',
        "computed_mixed": '.concat("a",.sprintf("%c",34),"b")',
    }
    out = {
        f"{action}_{condition}_{name}": f".byte 6\n.assert {condition},{action}"
        + ("" if message is None else "," + message)
        + "\n.byte 7\n"
        for action in ["error", "warning", "lderror", "ldwarning"]
        for condition in [0, 1]
        for name, message in messages.items()
    }
    for name, condition in {
        "comma_char": "','=44",
        "apostrophe_char": "'" * 3 + "=39",
        "open_paren_char": "'('=40",
    }.items():
        for action in ["error", "warning"]:
            for message in ['"a"', '"a,b"']:
                out[f"{name}_{action}_{message}"] = (
                    f".byte 6\n.assert {condition},{action},{message}\n.byte 7\n"
                )
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
