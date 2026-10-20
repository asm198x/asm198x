"""Measure character-token boundaries in native ca65 data operands."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent


def cases() -> dict[str, str]:
    out = {}
    for cpu in ["6502", "65816", "huc6280"]:
        for directive in ["byte", "word", "dbyt", "dword"]:
            for byte in [44, 34, 39, 92, 59, 40, 41]:
                character = "'" + chr(byte) + "'"
                for form, operand in {
                    "lone": character,
                    "pair": character + ",7",
                    "arithmetic": character + "+1,7",
                    "call": ".max(" + character + ",7),7",
                }.items():
                    line = "." + directive + " " + operand + "\n"
                    for context, source in {
                        "direct": line,
                        "macro": ".macro emit\n" + line + ".endmacro\nemit\n",
                    }.items():
                        name = f"{cpu}_{directive}_{byte}_{form}_{context}"
                        out[name] = f'.setcpu "{cpu}"\n.byte 6\n' + source + ".byte 7\n"
    for cpu in ["6502", "65816", "huc6280"]:
        for directive in ["byte", "word", "dbyt", "dword"]:
            for name, operand in {
                "empty": "1,,7",
                "trailing_comma": "1,",
                "leading_comma": ",1",
                "unmatched_open": "(1,7",
                "unmatched_close": "1),7",
                "bad_character": "'ab'",
                "open_string": '"open',
                "character_tail": "','x",
            }.items():
                line = "." + directive + " " + operand + "\n"
                for context, source in {
                    "direct": line,
                    "macro": ".macro emit\n" + line + ".endmacro\nemit\n",
                }.items():
                    out[f"{cpu}_{directive}_invalid_{name}_{context}"] = (
                        f'.setcpu "{cpu}"\n.byte 6\n' + source + ".byte 7\n"
                    )
    assert len(out) == 864
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
