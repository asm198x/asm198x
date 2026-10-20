"""Native numeric-index scope and reference boundaries for .strat."""

import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    return {
        "scoped_shadow": 'INDEX=0\n.scope inner\nINDEX=1\n.byte .strat("AB",INDEX)\n.endscope\n',
        "named_reference": 'INDEX=0\n.byte .strat("AB",INDEX)\n.byte .referenced(INDEX)\n',
        "literal_arithmetic": '.byte .strat("AB",1-1)\n',
        "nested_literal_index": '.byte .strat("AB",.strlen("AB")-1)\n',
    }


if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location(
        "capture", Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
    )
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
