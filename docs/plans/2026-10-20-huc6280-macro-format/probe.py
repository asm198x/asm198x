"""Measure ordinary ca65-family macro programs before formatter changes."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent


def cases() -> dict[str, str]:
    programs = {
        "no_arguments": ".macro emit\n.byte 7\n.endmacro\nemit\n",
        "parameter": ".macro emit operand\n.byte operand\n.endmacro\nemit $FF\n",
        "two_parameters": ".macro emit first, second\n.byte first,second\n.endmacro\nemit 1,2\n",
        "quoted_parameter": '.macro emit operand\n.byte operand\n.endmacro\nemit "a,b"\n',
        "character_parameter": ".macro emit operand\n.byte operand\n.endmacro\nemit ','\n",
        "arithmetic": ".macro emit operand\n.byte operand+1\n.endmacro\nemit 6\n",
        "locals": ".macro emit\n.local here\nhere: .byte 7\n.word here\n.endmacro\nemit\nemit\n",
        "nested_calls": ".macro outer\ninner\n.endmacro\n.macro inner\n.byte 7\n.endmacro\nouter\n",
        "labelled_call": ".macro emit operand\n.byte operand\n.endmacro\nhere: emit 7\n.word here\n",
        "short_form": ".mac emit operand\n.byte operand\n.endmac\nemit 7\n",
        "keyword_case": ".MACRO Emit operand\n.byte operand\n.ENDMACRO\nEmit 7\n",
        "unused_values": ".macro unused operand\n.local here\nhere: .byte operand\n.res MISSING\n.endmacro\n.byte 7\n",
        "comments": '; before\n.macro emit operand ; header\n; body\n.byte operand ; value\n\n.endmacro ; close\nemit "a;b" ; call\n',
        "unused_repeat": ".macro unused count\n.repeat count,I\n.byte I\n.endrepeat\n.endmacro\n.byte 7\n",
        "unused_include": '.macro unused\n.include "missing.inc"\n.endmacro\n.byte 7\n',
        "multiple_calls": ".macro emit operand\n.byte operand\n.endmacro\nemit 7\nemit 8\n",
    }
    return {
        f"{cpu}_{name}": f'.setcpu "{cpu}"\n.org $1000\n.byte 6\n'
        + source
        + ".byte 8\n"
        for cpu in ["65816", "huc6280"]
        for name, source in programs.items()
    }


if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location(
        "capture", ROOT.parent / "2026-10-19-ca65-scope-binding/probe.py"
    )
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
