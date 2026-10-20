"""Retain native .sprintf argument typing and generated literal bytes."""

import importlib.util
from pathlib import Path

FILES = {"format.inc": '.byte .sprintf("%d","2")\n'}


def cases() -> dict[str, str]:
    out = {}

    def data(expr: str) -> str:
        return "VALUE=2\n.byte 6\n.byte " + expr + "\n.byte 7\n"

    values = ["2", '"2"', "VALUE", '"VALUE"', ".string(2)", "'2'", '("2")', '+"2"']
    for fmt in ["%d", "%u", "%x", "%X", "%c", "%s"]:
        for index, value in enumerate(values):
            out[f"format_{fmt}_{index}"] = data(f'.sprintf("{fmt}",{value})')
    out.update(
        {
            "mixed_first_string_number": data('.sprintf("%d%s","2","xy")'),
            "mixed_last_string_number": data('.sprintf("%s%d","xy","2")'),
            "mixed_valid": data('.sprintf("%d%s",2,"xy")'),
            "defined_string_number": '.define VALUE "2"\n.byte .sprintf("%d",VALUE)\n',
            "defined_number": '.define VALUE 2\n.byte .sprintf("%d",VALUE)\n',
            "captured_string_number": '.define VALUE "2"\n.macro emit\n.byte .sprintf("%d",VALUE)\n.endmacro\n.undefine VALUE\n.define VALUE 2\nemit\n',
            "argument_string_number": '.macro emit arg\n.byte .sprintf("%d",arg)\n.endmacro\nemit "2"\n',
            "argument_number": '.macro emit arg\n.byte .sprintf("%d",arg)\n.endmacro\nemit 2\n',
            "included": '.include "format.inc"\n',
            "inactive": '.if 0\n.byte .sprintf("%d","2")\n.endif\n.byte 7\n',
            "unused": '.macro unused\n.byte .sprintf("%d","2")\n.endmacro\n.byte 7\n',
            "zero_repeat": '.repeat 0\n.byte .sprintf("%d","2")\n.endrepeat\n.byte 7\n',
            "selected_repeat": '.repeat 1\n.byte .sprintf("%d","2")\n.endrepeat\n',
            "scoped_number": '.scope inner\nVALUE=2\n.byte .sprintf("%d",VALUE)\n.endscope\n',
            "constructed_quote": data('.sprintf("%c",34)'),
            "constructed_backslash": data('.sprintf("%c",92)'),
            "concat_backslash": data(r'.concat("a\b","c")'),
            "concat_two_backslashes": data(r'.concat("a\\b","c")'),
            "sprintf_string_backslash": data(r'.sprintf("%s","a\b")'),
            "sprintf_string_two_backslashes": data(r'.sprintf("%s","a\\b")'),
            "raw_quote_escape": data(r'"a\"b"'),
            "raw_adjacent_strings": data('"a""b"'),
            "raw_string_addition": data('"a"+"b"'),
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
