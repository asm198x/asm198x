"""Measure ca65 selector bounds as numeric expressions rather than strings."""

import importlib.util
from pathlib import Path

FILES = {"bounds.inc": '.byte .left("2",{"abcd"})\n'}


def cases() -> dict[str, str]:
    out = {}

    def data(expr: str) -> str:
        return "VALUE=2\n.byte 6\n.byte " + expr + "\n.byte 7\n"

    bounds = [
        "0",
        "1",
        "2",
        "VALUE",
        "VALUE-1",
        "-1",
        "$FFFFFFFF",
        '"0"',
        '"2"',
        '"VALUE"',
        "'2'",
        ".string(2)",
        '.ident("VALUE")',
        '("2")',
        '+"2"',
    ]
    lists = {"string": '{"abcd"}', "numbers": "{1,2,3}"}
    for function in ["left", "right"]:
        for bound in bounds:
            for kind, source in lists.items():
                out[f"{function}_{bound}_{kind}"] = data(
                    f".{function}({bound},{source})"
                )
    pairs = [
        ("0", "1"),
        ("1", "2"),
        ('"0"', "1"),
        ("0", '"2"'),
        ('"0"', '"2"'),
        (".string(0)", "1"),
        ("0", ".string(1)"),
        ("-1", "1"),
        ("1", "-1"),
        ("VALUE-1", "1"),
        ('("0")', "1"),
        ("0", '+"2"'),
    ]
    for start, count in pairs:
        for kind, source in lists.items():
            out[f"mid_{start}_{count}_{kind}"] = data(f".mid({start},{count},{source})")
    out.update(
        {
            "defined_string": '.define BOUND "2"\n.byte 6\n.byte .left(BOUND,{"abcd"})\n.byte 7\n',
            "defined_number": '.define BOUND 2\n.byte 6\n.byte .left(BOUND,{"abcd"})\n.byte 7\n',
            "captured_string": '.define BOUND "2"\n.macro emit\n.byte .left(BOUND,{"abcd"})\n.endmacro\n.undefine BOUND\n.define BOUND 2\nemit\n',
            "captured_number": '.define BOUND 2\n.macro emit\n.byte .left(BOUND,{"abcd"})\n.endmacro\n.undefine BOUND\n.define BOUND "2"\nemit\n',
            "argument_string": '.macro emit arg\n.byte .left(arg,{"abcd"})\n.endmacro\nemit "2"\n',
            "argument_number": '.macro emit arg\n.byte .left(arg,{"abcd"})\n.endmacro\nemit 2\n',
            "selected": '.if 1\n.byte .left("2",{"abcd"})\n.endif\n',
            "inactive": '.if 0\n.byte .left("2",{"abcd"})\n.endif\n.byte 7\n',
            "unused": '.macro unused\n.byte .left("2",{"abcd"})\n.endmacro\n.byte 7\n',
            "zero_repeat": '.repeat 0\n.byte .left("2",{"abcd"})\n.endrepeat\n.byte 7\n',
            "selected_repeat": '.repeat 1\n.byte .left("2",{"abcd"})\n.endrepeat\n',
            "included": '.include "bounds.inc"\n.byte 7\n',
            "numeric_scope": '.scope inner\nVALUE=2\n.byte .left(VALUE,{"abcd"})\n.endscope\n',
            "label_bound": 'place:.byte 6\n.byte .left(place,{"abcd"})\n',
            "nested_concat": data('.left(.concat("","2"),{"abcd"})'),
            "nested_sprintf": data('.mid(0,.sprintf("%d",2),{1,2,3})'),
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
