"""Measure ca65 function-name/argument boundaries, including nested calls."""

import importlib.util
from pathlib import Path

FILES = {"body.inc": '.ident ("mark"):.byte 7\n.word mark\n'}


def cases() -> dict[str, str]:
    out = {}
    patterns = {
        "ident_label": '.ident{gap}("mark"):.byte 7\n.word mark\n',
        "ident_scalar": 'mark=9\n.byte .ident{gap}("mark")\n',
        "concat": '.byte .concat{gap}("a","b")\n',
        "sprintf": '.byte .sprintf{gap}("%02d",7)\n',
        "strlen": '.byte .strlen{gap}("abc")\n',
        "left": '.byte .left{gap}(2,"abcd")\n',
        "tcount": ".byte .tcount{gap}({{a,b}})\n",
        "blank": ".byte .blank{gap}({{}})\n",
        "nested": '.byte .strlen{gap}(.concat{gap}("ab","c"))\n',
        "nested_ident": '.ident{gap}(.sprintf{gap}("row%d",2)):.byte 7\n.word row2\n',
        "bad_type": ".ident{gap}(7):.byte 7\n",
        "unknown": ".byte .UNKNOWN{gap}(7)\n",
        "unclosed": '.byte .strlen{gap}("abc"\n',
        "inactive": ".if 0\n.byte .strlen{gap}(missing)\n.endif\n.byte 8\n",
        "zero_repeat": ".repeat 0\n.byte .strlen{gap}(missing)\n.endrepeat\n.byte 8\n",
    }
    for index, gap in enumerate(["", " ", "\t", " \t  "]):
        for name, source in patterns.items():
            out[f"{name}_{index}"] = source.format(gap=gap)
        out[f"upper_ident_{index}"] = f'.IDENT{gap}("mark"):.byte 7\n.word mark\n'
        out[f"upper_nested_{index}"] = f'.byte .STRLEN{gap}(.CONCAT{gap}("ab","c"))\n'
    out.update(
        {
            "quoted_function": '.byte ".ident (7)"\n',
            "comment_function": ".byte 8; .ident (7)\n",
            "unread_function": ".byte 8\n.end\n.ident (7):.byte 7\n",
            "include_context": '.include "body.inc"\n',
            "ordinary_constant": "count = 4\n.byte count\n",
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
