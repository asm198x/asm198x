"""Check whether native unused self behavior is enum-specific."""

import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    for outer in ["", "RED=9\n"]:
        for opening, closing, name in [
            (".scope inner\n", ".endscope\n", "inner::RED"),
            (".enum Colors\n", ".endenum\n", "Colors::RED"),
        ]:
            for rhs in ["RED+1", "RED-RED"]:
                for use in [".byte 7\n", f".byte {name}\n"]:
                    out[f"{bool(outer)}_{opening.strip()}_{rhs}_{use.strip()}"] = (
                        outer + opening + f"RED={rhs}\n" + closing + use
                    )
    return out


if __name__ == "__main__":
    source = Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
    spec = importlib.util.spec_from_file_location("native_capture", source)
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
