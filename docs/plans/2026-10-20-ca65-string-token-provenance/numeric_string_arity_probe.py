"""Compare real empty calls with numeric calls captured as tokens."""

import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    for function in ["strlen", "strat"]:
        for index, args in enumerate(
            ["", '""', '"AB"', "0", '"AB",0', '"AB",1', '"AB",0,1', '"AB",']
        ):
            expression = f".{function}({args})"
            out[f"{function}_{index}"] = f".byte 6\n.byte {expression}\n.byte 7\n"
        empty = f".{function}()"
        for context, body in {
            "inactive": f".if 0\n.byte {empty}\n.endif\n",
            "unused_macro": f".macro emit\n.byte {empty}\n.endmacro\n",
            "zero_repeat": f".repeat 0\n.byte {empty}\n.endrepeat\n",
            "token_count": f".byte .tcount({{{empty}}})\n",
            "selected_tokens": f".byte .left(3,{{{empty}}})\n",
        }.items():
            out[f"{function}_{context}"] = ".byte 6\n" + body + ".byte 7\n"
    return out


if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location(
        "capture", Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
    )
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
