"""Retain native included-macro range error locations."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent
FILES = {"broken.inc": ".macro emit operand\n.byte operand\n.endmacro\nemit 256\n"}


if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location(
        "capture", ROOT.parent / "2026-10-19-ca65-scope-binding/probe.py"
    )
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.FILES = FILES
    runner.cases = lambda: {
        cpu: f'.setcpu "{cpu}"\n.org $1000\n.include "broken.inc"\n'
        for cpu in ["65816", "huc6280"]
    }
    runner.main()
