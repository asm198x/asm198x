"""Measure physical quoted-token admission without selecting meaning."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent
REPO = ROOT.parents[2]


def cases() -> dict[str, str]:
    forms = {
        "char_single_quote": "'",
        "char_two_quotes": "'" * 2,
        "char_three_quotes": "'" * 3,
        "char_open_a": "'A",
        "char_closed_a": "'A'",
        "char_open_ab": "'AB",
        "char_closed_ab": "'AB'",
        "char_digits": "'4294967300'",
        "char_utf8": "'é'",
        "char_slash": "'\\'",
        "char_slash_n": "'\\n'",
        "string_plain": '"a"',
        "string_empty": '""',
        "string_slash": '"a\\b"',
        "string_trailing_slash": '"a\\"',
        "string_unterminated": '"a',
        "string_adjacent": '"a""b"',
        "string_escaped_quote": '"a\\"b"',
        "string_numeric": '"4294967300000000"',
    }
    out = {}
    for name, value in forms.items():
        line = f".byte {value}\n"
        for context, body in {
            "direct": line,
            "labelled": "here: " + line,
            "selected": ".if 1\n" + line + ".endif\n",
            "inactive": ".if 0\n" + line + ".endif\n",
            "unused_macro": ".macro emit\n" + line + ".endmacro\n",
            "zero_repeat": ".repeat 0\n" + line + ".endrepeat\n",
            "after_end": ".end\n" + line,
        }.items():
            out[f"{context}_{name}"] = ".byte 6\n" + body + ".byte 7\n"
    for name, value in {"character": "'4294967300'", "string": '"a'}.items():
        line = f".byte {value}\n"
        out[f"missing_include_before_{name}"] = '.include "missing.inc"\n' + line
        out[f"bad_{name}_before_missing_include"] = line + '.include "missing.inc"\n'
        out[f"inactive_end_before_{name}"] = ".if 0\n.end\n.endif\n" + line
    return out


if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location(
        "capture", REPO / "docs/plans/2026-10-19-ca65-scope-binding/probe.py"
    )
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
