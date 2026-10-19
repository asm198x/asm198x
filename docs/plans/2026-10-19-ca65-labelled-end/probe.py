"""Measure label binding before native termination, including refusal boundaries."""

import importlib.util
from pathlib import Path

FILES = {"ending.inc": ".word finish\nfinish:.end,$100000000\n.byte 8\n"}


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def cases() -> dict[str, str]:
    old = load(
        "old",
        Path(__file__).parents[1] / "2026-10-19-ca65-end-token-boundaries/probe.py",
    )
    out = {"named": ".word finish\nfinish:.end\n.byte 8\n"}
    out.update(old.cases())
    for cpu in ["6502", "4510"]:
        for name in ["A", "X", "Y", "Z", "F", "S", "SP", "lda", "LDA", "entry"]:
            for form, head in [
                ("plain", name + ":"),
                ("space", name + " :"),
                ("ident", f'.ident("{name}"):'),
            ]:
                out[f"{cpu}_{name}_{form}"] = (
                    f'.setcpu "{cpu}"\n.byte 7\n{head}.end\n.byte 8\n'
                )
    out.update(
        {
            "nested_generated_tail": '.word finish\n.ident(.concat("fin","ish")):.end .sprintf("%bad",0)\n.byte 8\n',
            "generated_bad_head": ".ident(UNDEFINED):.end\n.byte 8\n",
            "generated_longer_token": '.ident("finish"):.end2\n.byte 8\n',
            "generated_quoted_colon": '.ident("fin:ish"):.end\n.byte 8\n',
            "duplicate_label": "finish:.byte 7\nfinish:.end\n.byte 8\n",
            "constant_collision": "finish=7\nfinish:.end\n.byte 8\n",
            "scope_end": ".scope outer\n.word finish\nfinish:.end\n.endscope\n.byte 8\n",
            "proc_end": ".proc outer\n.word finish\nfinish:.end\n.endproc\n.byte 8\n",
            "conditional_end": ".if 1\n.byte 7\nfinish:.end\n.endif\n.byte 8\n",
            "dead_end_label": ".if 0\nfinish:.end\n.else\n.byte .defined(finish)\n.endif\n",
        }
    )
    return out


if __name__ == "__main__":
    runner = load(
        "capture", Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
    )
    runner.cases = cases
    runner.FILES = FILES
    runner.main()
