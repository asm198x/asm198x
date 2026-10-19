"""Measure selected cheap-local anchor failures and native anchor events."""

import importlib.util
from pathlib import Path

FILES = {"anchor.inc": "anchor:.byte 6\n", "cheap.inc": "@near:.byte 7\n.word @near\n"}


def cases() -> dict[str, str]:
    prefixes = {
        "root": ("", ""),
        "label": ("anchor:.byte 6\n", ""),
        "constant": ("anchor=6\n", ""),
        "anonymous": (":.byte 6\n", ""),
        "origin": (".org $1000\n", ""),
        "scope": (".scope inner\n", ".endscope\n"),
        "proc": (".proc inner\n", ".endproc\n"),
        "unnamed_scope": (".scope\n", ".endscope\n"),
        "scope_after_label": ("anchor:.byte 6\n.scope inner\n", ".endscope\n"),
        "empty_scope_before": (".scope inner\n.endscope\n", ""),
        "scope_output_before": (".scope inner\n.byte 6\n.endscope\n", ""),
        "segment_before": ('.segment "CODE"\n', ""),
        "macro_before": (".macro emit\n.byte 6\n.endmacro\n", ""),
        "macro_emits_before": (".macro emit\n.byte 6\n.endmacro\nemit\n", ""),
        "repeat_emits_before": (".repeat 1\n.byte 6\n.endrepeat\n", ""),
        "taken_label": (".if 1\nanchor:.byte 6\n.endif\n", ""),
        "skipped_label": (".if 0\nanchor:.byte 6\n.endif\n", ""),
        "zero_repeat_label": (".repeat 0\nanchor:.byte 6\n.endrepeat\n", ""),
        "included_anchor": ('.include "anchor.inc"\n', ""),
    }
    out = {}
    for prefix_name, (before, after) in prefixes.items():
        for form, name in [("plain", "@near"), ("generated", '.ident("@near")')]:
            bodies = {
                "label": f"{name}:.byte 7\n.word {name}\n",
                "constant": f"{name}=7\n.byte {name}\n",
                "defined": f".byte .defined({name})\n",
                "referenced": f".byte .referenced({name})\n",
                "const": f".byte .const({name})\n",
                "ifdef": f".ifdef {name}\n.byte 7\n.else\n.byte 8\n.endif\n",
                "ifconst": f".ifconst {name}\n.byte 7\n.else\n.byte 8\n.endif\n",
                "ifnref": f".ifnref {name}\n.byte 7\n.else\n.byte 8\n.endif\n",
            }
            for kind, body in bodies.items():
                out[f"{prefix_name}_{form}_{kind}"] = before + body + after
    for name in ["1", "1near", "_", ".near", "a.b"]:
        out[f"anchored_plain_{name}"] = (
            f"anchor:.byte 6\n@{name}:.byte 7\n.word @{name}\n"
        )
        out[f"anchored_generated_{name}"] = (
            f'anchor:.byte 6\n.ident("@{name}"):.byte 7\n.word .ident("@{name}")\n'
        )
    out["dot_label_only"] = "anchor:.byte 6\n@.near:.byte 7\n"
    out["inner_dot_label_only"] = "anchor:.byte 6\n@a.b:.byte 7\n"
    out.update(
        {
            "root_expression": ".word @near\n",
            "root_ref_directive": ".ref @near\n.byte 8\n",
            "root_if_head": "@near:.if 0\n.byte 7\n.endif\n",
            "root_repeat_head": "@near:.repeat 0\n.byte 7\n.endrepeat\n",
            "root_macro_definition_head": "@near:.macro emit\n.byte 7\n.endmacro\n.byte 8\n",
            "unused_macro": ".macro emit\n@near:.byte 7\n.endmacro\n.byte 8\n",
            "called_macro": ".macro emit\n@near:.byte 7\n.endmacro\nemit\n",
            "zero_repeat": ".repeat 0\n@near:.byte 7\n.endrepeat\n.byte 8\n",
            "inactive": ".if 0\n@near:.byte 7\n.endif\n.byte 8\n",
            "unread": ".byte 8\n.end\n@near:.byte 7\n",
            "macro_delayed_include_anchor": '.macro emit\n.include "cheap.inc"\nanchor:.byte 6\n.endmacro\nemit\n',
            "repeat_delayed_include_anchor": '.repeat 1\n.include "cheap.inc"\nanchor:.byte 6\n.endrepeat\n',
            "include_missing_anchor": '.include "cheap.inc"\n',
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
