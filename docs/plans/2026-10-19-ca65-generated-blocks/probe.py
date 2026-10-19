"""Measure selected generated block heads without evaluating skipped source."""

import importlib.util
from pathlib import Path

FILES = {"body.inc": ".byte 9\n"}


def block(head: str, directive: str) -> str:
    close = ".endrepeat" if directive.startswith(".repeat") else ".endif"
    return f"{head}:{directive}\n.byte 7\n{close}\n.byte 8\n"


def cases() -> dict[str, str]:
    out = {"generated_if": block('.ident("mark")', ".if 1")}
    for kind, head in [
        ("plain", "mark"),
        ("generated", '.ident("mark")'),
        ("nested", '.ident(.concat("ma", "rk"))'),
    ]:
        for directive in [".if 0", ".if 1", ".repeat 0", ".repeat 2"]:
            out[f"{kind}_{directive}"] = block(head, directive)
    for cpu in ["6502", "4510"]:
        for name in ["A", "X", "Y", "S", "SP", "Z", "F", "LDA"]:
            for kind, head in [("plain", name), ("generated", f'.ident("{name}")')]:
                for directive in [".if 1", ".repeat 1"]:
                    out[f"{cpu}_{name}_{kind}_{directive}"] = (
                        f'.setcpu "{cpu}"\n' + block(head, directive)
                    )
    out.update(
        {
            "forward_address": '.word mark\n.ident("mark"):.if 1\n.byte 7\n.endif\n',
            "repeat_address": '.word mark\n.ident("mark"):.repeat 2\n.byte 7\n.endrepeat\n',
            "generated_defined": '.ident("mark"):.ifdef mark\n.byte 7\n.else\n.byte 8\n.endif\n',
            "generated_unreferenced": '.ident("mark"):.ifnref mark\n.byte 7\n.else\n.byte 8\n.endif\n',
            "repeat_counter": '.ident("mark"):.repeat 2,I\n.byte I\n.endrepeat\n.word mark\n',
            "nested_outer_false": '.if 0\n.ident("mark"):.if 1\n.byte 7\n.endif\n.endif\n.byte .defined(mark)\n',
            "nested_unknown_head": '.if 0\n.ident(.sprintf("%bad",0)):.if 1\n.byte 7\n.endif\n.endif\n.byte 8\n',
            "zero_unknown_head": '.repeat 0\n.ident(.sprintf("%bad",0)):.if 1\n.byte 7\n.endif\n.endrepeat\n.byte 8\n',
            "inactive_missing_include": '.if 0\n.ident("mark"):.if 1\n.include "missing.inc"\n.endif\n.endif\n.byte 8\n',
            "active_include": '.ident("mark"):.if 1\n.include "body.inc"\n.endif\n.word mark\n',
            "macro_context": '.macro emit\n.ident("mark"):.if 1\n.byte 7\n.endif\n.endmacro\nemit\n.word mark\n',
            "scope_context": '.scope outer\n.ident("mark"):.if 1\n.byte 7\n.endif\n.endscope\n.word outer::mark\n',
            "iteration_names": '.repeat 2,I\n.ident(.sprintf("row%d",I)):.if 1\n.byte I\n.endif\n.endrepeat\n.word row0,row1\n',
            "invalid_selected_name": '.ident("bad;name"):.if 1\n.byte 7\n.endif\n',
            "invalid_selected_argument": ".ident(7):.repeat 0\n.byte 7\n.endrepeat\n",
            "inactive_oversized_argument": '.if 0\n.ident(.sprintf("%d",$100000000)):.if 1\n.byte 7\n.endif\n.endif\n',
            "static_labelled_endif": ".if 1\n.byte 7\ntail:.endif\n.word tail\n",
            "static_labelled_endrepeat": ".repeat 1\n.byte 7\ntail:.endrepeat\n.word tail\n",
            "static_labelled_else": ".if 1\n.byte 7\ntail:.else\n.byte 8\n.endif\n.word tail\n",
            "labelled_endif": '.if 1\n.byte 7\n.ident("tail"):.endif\n.word tail\n',
            "labelled_endrepeat": '.repeat 1\n.byte 7\n.ident("tail"):.endrepeat\n.word tail\n',
            "labelled_else": '.if 1\n.byte 7\n.ident("tail"):.else\n.byte 8\n.endif\n.word tail\n',
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
