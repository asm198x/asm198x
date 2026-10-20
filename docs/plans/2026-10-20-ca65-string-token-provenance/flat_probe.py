"""Measure byte construction with native 65816 and HuC6280 CPU selection."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def cases() -> dict[str, str]:
    original = load(
        "byte_cases",
        ROOT.parent / "2026-10-20-ca65-format-argument-types/byte_probe.py",
    ).cases()
    return {
        f"{cpu}_{name}": f'.setcpu "{cpu}"\n' + source
        for cpu in ["65816", "huc6280"]
        for name, source in original.items()
    }


if __name__ == "__main__":
    runner = load("capture", ROOT.parent / "2026-10-19-ca65-scope-binding/probe.py")
    runner.cases = cases
    runner.main()
