"""Measure .ident lexical names independently of their statement context."""

import importlib.util
import json
from pathlib import Path

NAMES = [
    "mark",
    "_",
    "_a",
    "a1",
    "A",
    "LDA",
    "@near",
    "@_x",
    ".dot",
    "a.b",
    "a$b",
    "a@b",
    "@",
    "@@name",
    "1bad",
    " mark",
    "mark ",
    "bad name",
    "",
    "mé",
    "m_9",
    "__",
    "$x",
    "a-b",
    ":",
    "::mark",
]


def cases() -> dict[str, str]:
    out = {}
    for index, name in enumerate(NAMES):
        for gap_index, gap in enumerate(["", " "]):
            ident = f".ident{gap}({json.dumps(name, ensure_ascii=False)})"
            for kind, body in [
                ("label", f"{ident}:.byte 7\n"),
                ("query", f".byte .defined({ident})\n"),
                ("constant", f"{ident}=7\n.byte {ident}\n"),
            ]:
                out[f"{kind}_{index}_{gap_index}"] = "anchor:.byte 6\n" + body
    out.update(
        {
            "plain_missing_anchor": "@near:.byte 7\n.word @near\n",
            "generated_missing_anchor": '.ident("@near"):.byte 7\n.word .ident("@near")\n',
            "generated_with_anchor": 'anchor:.byte 6\n.ident("@near"):.byte 7\n.word .ident("@near")\n',
            "inactive_invalid": '.if 0\n.byte .ident(" mark")\n.endif\n.byte 8\n',
            "unused_invalid_macro": '.macro emit\n.byte .ident(" mark")\n.endmacro\n.byte 8\n',
            "zero_repeat_invalid": '.repeat 0\n.byte .ident(" mark")\n.endrepeat\n.byte 8\n',
            "selected_invalid_macro": '.macro emit\n.byte .ident(" mark")\n.endmacro\nemit\n',
            "nested_concat_invalid": '.ident(.concat(" ","mark")):.byte 7\n',
            "nested_concat_valid": '.ident(.concat("ma","rk")):.byte 7\n.word mark\n',
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
    runner.FILES = {}
    runner.main()
