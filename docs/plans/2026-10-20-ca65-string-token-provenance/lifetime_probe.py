"""Native byte-string construction through selected source and captures."""

import importlib.util
from pathlib import Path

FILES = {
    "byte.s": '.byte .sprintf("%c",255)\n',
    "macro.s": ".macro emit value\n.byte value\n.endmacro\n",
    "nested.s": '.include "byte.s"\n.byte .sprintf("%c",34)\n',
}


def cases() -> dict[str, str]:
    out = {}
    for name, value in {
        "quote": '.sprintf("%c",34)',
        "slash": '.sprintf("%c",92)',
        "high": '.sprintf("%c",255)',
        "partial": '.sprintf("%.1s","é")',
    }.items():
        for context, source in {
            "macro_argument": f".macro emit value\n.byte value\n.endmacro\nemit {value}\n",
            "macro_body": f".macro emit\n.byte {value}\n.endmacro\nemit\n",
            "nested_macro": f".macro inner value\n.byte value\n.endmacro\n.macro outer value\ninner value\n.endmacro\nouter {value}\n",
            "repeat": f".repeat 2\n.byte {value}\n.endrepeat\n",
            "selected": f".if 1\n.byte {value}\n.else\n.byte 0\n.endif\n",
            "skipped": f".if 0\n.byte {value}\n.else\n.byte 7\n.endif\n",
            "asciiz": f".asciiz {value}\n",
        }.items():
            out[f"{context}_{name}"] = source
    out.update(
        {
            "include": '.include "byte.s"\n.byte 7\n',
            "nested_include": '.include "nested.s"\n.byte 7\n',
            "included_macro": '.include "macro.s"\nemit .sprintf("%c",255)\n',
            "repeat_counter": '.repeat 2,I\n.byte .sprintf("%c",I+254)\n.endrepeat\n',
            "captured_text": '.define TEXT "A"\n.macro emit\n.byte .sprintf("%s",TEXT)\n.endmacro\n.undefine TEXT\n.define TEXT "B"\nemit\n.byte .sprintf("%s",TEXT)\n',
            "unused_bad_character": '.macro emit\n.byte .sprintf("%c",0)\n.endmacro\n.byte 7\n',
            "zero_repeat_bad_character": '.repeat 0\n.byte .sprintf("%c",0)\n.endrepeat\n.byte 7\n',
            "skipped_bad_character": '.if 0\n.byte .sprintf("%c",0)\n.endif\n.byte 7\n',
        }
    )
    return out


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
