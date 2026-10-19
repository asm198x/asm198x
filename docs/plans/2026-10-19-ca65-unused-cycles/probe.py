"""Measure when native validates unused circular alias values."""

import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    for value in [None, "9", "$1234"]:
        for when in ["before", "after"]:
            outer = f"RED={value}\n" if value is not None else ""
            for graph in [
                "RED=RED+1\n",
                "RED=GREEN+1\nGREEN=RED+1\n",
                "RED=GREEN+1\nGREEN=BLUE+1\nBLUE=RED+1\n",
            ]:
                for use in [
                    ".byte 7\n",
                    ".word inner::RED\n",
                    "OTHER=inner::RED\n.byte 7\n",
                    ".byte .defined(inner::RED),.ref(inner::RED)\n",
                ]:
                    source = (
                        (outer if when == "before" else "")
                        + ".scope inner\n"
                        + graph
                        + ".endscope\n"
                        + (outer if when == "after" else "")
                        + use
                    )
                    out[f"{value}_{when}_{graph.count('=')}_{use.strip()}"] = source
    return out


if __name__ == "__main__":
    source = Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
    spec = importlib.util.spec_from_file_location("native_capture", source)
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
