"""Measure selected and forward generated labels in native constancy queries."""

import importlib.util
from pathlib import Path

FILES = {"label.inc": '.ident("mark"):.byte 6\n'}


def cases() -> dict[str, str]:
    out = {}
    for kind, head in [
        ("static", "mark"),
        ("generated", '.ident("mark")'),
        ("concat", '.ident(.concat("ma","rk"))'),
        ("spaced", '.ident ( "mark" )'),
    ]:
        definition = f"{head}:.byte 6\n"
        out[f"{kind}_after"] = definition + ".byte .const(mark)\n"
        out[f"{kind}_before"] = ".byte .const(mark)\n" + definition
        out[f"{kind}_macro_after"] = (
            ".macro emit\n.byte .const(mark)\n.endmacro\n" + definition + "emit\n"
        )
        out[f"{kind}_repeat_after"] = (
            definition + ".repeat 1\n.byte .const(mark)\n.endrepeat\n"
        )
        out[f"{kind}_scope_after"] = (
            ".scope inner\n" + definition + ".byte .const(mark)\n.endscope\n"
        )
    out.update(
        {
            "conditional_head": '.ident("mark"):.if 1\n.byte 6\n.endif\n.byte .const(mark)\n',
            "repeat_head": '.ident("mark"):.repeat 1\n.byte 6\n.endrepeat\n.byte .const(mark)\n',
            "included_after": '.include "label.inc"\n.byte .const(mark)\n',
            "included_before": '.byte .const(mark)\n.include "label.inc"\n',
            "missing": ".byte .const(mark)\n",
            "skipped_generated": '.if 0\n.ident("mark"):.byte 6\n.endif\n.byte .const(mark)\n',
            "skipped_static": ".if 0\nmark:.byte 6\n.endif\n.byte .const(mark)\n",
            "unused_generated_macro": '.macro emit\n.ident("mark"):.byte 6\n.endmacro\n.byte .const(mark)\n',
            "selected_generated_macro": '.macro emit\n.ident("mark"):.byte 6\n.endmacro\nemit\n.byte .const(mark)\n',
            "constant_after": "mark=7\n.byte .const(mark)\n",
            "constant_before": ".byte .const(mark)\nmark=7\n",
            "invalid_generated_name": '.ident(" mark"):.byte 6\n.byte .const(mark)\n',
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
