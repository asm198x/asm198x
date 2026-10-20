"""Run the literal audit with each native flat CPU selected explicitly."""

import importlib.util
from pathlib import Path
from types import ModuleType

ROOT = Path(__file__).parent


def load(name: str, path: Path) -> ModuleType:
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


if __name__ == "__main__":
    literals = load("literals", ROOT / "literal_boundary_probe.py")
    runner = load("capture", ROOT.parent / "2026-10-19-ca65-scope-binding/probe.py")
    runner.cases = lambda: {
        f"{dialect}_{name}": f'.setcpu "{cpu}"\n' + source
        for dialect, cpu in [("65816", "65816"), ("huc6280", "huc6280")]
        for name, source in literals.cases().items()
    }
    runner.main()
