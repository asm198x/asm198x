"""Measure single literal boundaries without widening inactive lexical reading."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent


def values() -> dict[str, str]:
    return {
        "plain": '"a"',
        "empty": '""',
        "slash": r'"a\b"',
        "two_slashes": r'"a\\b"',
        "trailing_slash": r'"a\"',
        "trailing_two_slashes": r'"a\\"',
        "adjacent": '"a""b"',
        "separated": '"a" "b"',
        "addition": '"a"+"b"',
        "escaped_quote": r'"a\"b"',
        "double_slash_quote": r'"a\\"b"',
        "trailing_name": '"a"junk',
        "trailing_number": '"a"+1',
        "parenthesized": '("a")',
        "comma_number": '"a",1',
        "comma_string": '"a","b"',
        "comment": '"a" ; tail',
        "slash_comment": r'"a\" ; tail',
        "utf8": '"é"',
        "punctuation": '"a;,()b"',
        "empty_adjacent": '""""',
        "unterminated": '"a',
    }


def cases() -> dict[str, str]:
    out = {}
    for name, value in values().items():
        for directive in ["byte", "asciiz"]:
            line = f".{directive} {value}\n"
            for context, body in {
                "direct": line,
                "labelled": "here: " + line,
                "selected": ".if 1\n" + line + ".endif\n",
                "macro_body": ".macro emit\n" + line + ".endmacro\nemit\n",
                "inactive": ".if 0\n" + line + ".endif\n",
                "unused_macro": ".macro emit\n" + line + ".endmacro\n",
                "zero_repeat": ".repeat 0\n" + line + ".endrepeat\n",
            }.items():
                out[f"{context}_{directive}_{name}"] = ".byte 6\n" + body + ".byte 7\n"
        for function, expression in {
            "concat": f'.concat({value},"Z")',
            "sprintf": f'.sprintf("%s",{value})',
        }.items():
            out[f"{function}_{name}"] = f".byte 6\n.byte {expression}\n.byte 7\n"
    out["computed_quote"] = '.byte 6\n.byte .sprintf("%c",34)\n.byte 7\n'
    out["computed_slash"] = '.byte 6\n.byte .sprintf("%c",92)\n.byte 7\n'
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
