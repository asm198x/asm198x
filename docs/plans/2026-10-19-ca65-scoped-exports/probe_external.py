"""Check explicit same-object exports that satisfy scoped external references."""

import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "capture", Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
)
assert spec is not None and spec.loader is not None
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def cases() -> dict[str, str]:
    out = {}
    for word in ["import", "importzp", "global", "globalzp"]:
        for value in ["2", "$1234"]:
            for use in [".word RED", "lda RED"]:
                out[f"external_{word}_{value}_{use}"] = (
                    f"RED={value}\n.export RED\n.scope inner\n.{word} RED\n{use}\n.endscope\n"
                )
    for first in ["import", "global", "export"]:
        for second in ["import", "global", "export"]:
            out[f"mixed_{first}_{second}"] = (
                f"RED=2\n.export RED\n.scope inner\n.{first} RED\n.{second} RED\n.byte RED\n.endscope\n"
            )
    return out


runner.cases = cases
runner.main()
