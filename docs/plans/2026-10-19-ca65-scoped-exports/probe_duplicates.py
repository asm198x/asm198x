"""Check exports that share an external spelling across lexical declarations."""

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
    for first in ["export", "global"]:
        for second in ["export", "global"]:
            for root in ["before", "after", "local"]:
                body = ("RED=2\n" if root == "before" else "") + ".scope first\n"
                body += (
                    "RED=2\n" if root == "local" else ""
                ) + f".{first} RED\n.endscope\n.scope second\n"
                body += (
                    "RED=3\n" if root == "local" else ""
                ) + f".{second} RED\n.endscope\n"
                body += "RED=2\n" if root == "after" else ""
                out[f"{first}_{second}_{root}"] = body + ".byte 7\n"
    return out


runner.cases = cases
runner.main()
