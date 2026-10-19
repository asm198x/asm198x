"""Separate allocation queries from the existing scoped constancy-query gap."""

import importlib.util
import json
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "capture", Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
)
assert spec is not None and spec.loader is not None
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def cases() -> dict[str, str]:
    original = json.loads(Path(__file__).with_name("native.json").read_text())
    out = {
        "allocation_" + c["name"]: c["source"].replace(",.const(RED)", "")
        for c in original["cases"]
        if c["name"].split("_")[0] in ["export", "exportzp", "global", "globalzp"]
        and "queries_" in c["name"]
    }
    for word in ["export", "exportzp"]:
        for expr in ["2", "BLUE", "*"]:
            for scoped in [False, True]:
                body = f".{word} RED := {expr}\nBLUE=2\nlda RED\n"
                out[f"width_{word}_{expr}_{scoped}"] = (
                    ".scope inner\n" + body + ".endscope\n" if scoped else body
                )
    assert len(out) == 20
    return out


runner.cases = cases
runner.main()
