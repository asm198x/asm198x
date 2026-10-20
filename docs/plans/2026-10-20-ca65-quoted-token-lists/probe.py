"""Measure ca65 quoted token-list admission and literal byte boundaries."""

import importlib.util
from pathlib import Path

FILES = {"quoted.inc": '.byte .left(2,"abcd")\n'}


def cases() -> dict[str, str]:
    forms = {
        "quoted": '"abcd"',
        "braced_quoted": '{"abcd"}',
        "empty_string": '""',
        "braced_empty_string": '{""}',
        "empty_list": "{}",
        "comma": '"a,b"',
        "parentheses": '"(a)"',
        "colon": '"F:"',
        "one_backslash": r'"a\b"',
        "two_backslashes": r'"a\\b"',
        "escaped_quote": r'"a\"b"',
        "adjacent_strings": '"a""b"',
        "bare_name": "VALUE",
        "number": "7",
        "wrapped": '("ab")',
        "single_char": "'a'",
        "nested": '.concat("ab","cd")',
        "braced_numbers": "{1,2,3}",
        "utf8": '"é"',
    }
    out = {}

    def data(expr: str) -> str:
        return "VALUE=9\n.byte 6\n.byte " + expr + "\n.byte 7\n"

    for function in ["left", "right"]:
        for count in [
            "-1",
            "0",
            "1",
            "2",
            "5",
            "$7FFFFFFF",
            "$80000000",
            "$FFFFFFFF",
            '"0"',
            '"2"',
        ]:
            for name, arg in forms.items():
                out[f"{function}_{count}_{name}"] = data(f".{function}({count},{arg})")
    for start, count in [
        ("-1", "1"),
        ("0", "-1"),
        ("0", "0"),
        ("0", "1"),
        ("0", "2"),
        ("1", "1"),
        ("2", "5"),
        ("$FFFFFFFF", "1"),
        ('"0"', "1"),
    ]:
        for name, arg in forms.items():
            out[f"mid_{start}_{count}_{name}"] = data(f".mid({start},{count},{arg})")
    for function in ["tcount", "blank"]:
        for name, arg in forms.items():
            out[f"{function}_{name}"] = data(f".{function}({arg})")
    for function in ["match", "xmatch"]:
        for index, (left, right) in enumerate(
            [
                ('"a"', '"b"'),
                ('"a"', '"a"'),
                ('""', '"a"'),
                ('"a"', '{"a"}'),
                ('{"a"}', '"a"'),
                ('""', "{}"),
                (r'"a\b"', r'{"a\b"}'),
                (r'"a\b"', r'"a\\b"'),
            ]
        ):
            out[f"{function}_{index}"] = data(f".{function}({left},{right})")
    out.update(
        {
            "spaced": data('.left ( 2, "abcd" )'),
            "tabbed": data('.left\t(2,"abcd")'),
            "nested_left": data('.left(1,.left(2,"abcd"))'),
            "nested_mid": data('.right(1,.mid(0,2,"abcd"))'),
            "nested_backslash": data(r'.left(1,.concat("a\b","c"))'),
            "nested_two_backslashes": data(r'.left(1,.concat("a\\b","c"))'),
            "format_quote": data('.left(1,.sprintf("%c",34))'),
            "bare_backslash": data(r'"a\b"'),
            "bare_two_backslashes": data(r'"a\\b"'),
            "defined_string": '.define VALUE "abcd"\n.byte 6\n.byte .left(2,VALUE)\n.byte 7\n',
            "captured_define": '.define VALUE "abcd"\n.macro emit\n.byte .left(2,VALUE)\n.endmacro\n.undefine VALUE\n.define VALUE "xy"\n.byte 6\nemit\n.byte 7\n',
            "argument": '.macro emit arg\n.byte .left(2,arg)\n.endmacro\n.byte 6\nemit "abcd"\n.byte 7\n',
            "repeat": '.byte 6\n.repeat 2\n.byte .left(2,"abcd")\n.endrepeat\n.byte 7\n',
            "included": '.byte 6\n.include "quoted.inc"\n.byte 7\n',
            "inactive": '.if 0\n.byte .left(2,"abcd")\n.endif\n.byte 7\n',
            "unused": '.macro unused\n.byte .left(2,"abcd")\n.endmacro\n.byte 7\n',
            "zero_repeat": '.repeat 0\n.byte .left(2,"abcd")\n.endrepeat\n.byte 7\n',
            "inactive_malformed": ".if 0\n.byte .left()\n.endif\n.byte 7\n",
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
