"""Capture native ca65 unary plus, including generated fields and inert tokens."""

import importlib.util
import json
import sys
from pathlib import Path
from types import ModuleType

ROOT = Path(__file__).resolve().parents[3]


def load(name: str, path: Path) -> ModuleType:
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


empty = load("empty", ROOT / "docs/plans/2026-10-21-ca65-empty-calls/probe.py")
BODIES = {
    "positive_byte": ".byte +1",
    "positive_product": ".byte 2*+3",
    "positive_group": ".byte -(+1)+2",
    "positive_fill": ".res 1,+1",
    "generated_fill": ".res .left(2,{1,2})+1",
    "captured_plus": ".byte .tcount({+1}),.xmatch({+1},{+1})",
    "missing_value": ".byte +",
    "positive_character": ".byte +'A'",
    "positive_query": ".byte +.const(VALUE)",
}


def cases() -> dict[str, str]:
    out: dict[str, str] = {}
    for dialect in ("ca65", "65816", "huc6280"):
        for name, body in BODIES.items():
            empty.add_contexts(out, dialect, name, "VALUE=2\n" + body + "\n")
    assert len(out) == 135
    return out


def main() -> None:
    capture = load(
        "capture", ROOT / "docs/plans/2026-10-19-ca65-scope-binding/probe.py"
    )
    prepared = cases()
    capture.cases = lambda: prepared
    capture.FILES = empty.FILES
    capture.main()
    path = Path(sys.argv[1])
    facts = json.loads(path.read_text())
    assert len(facts["cases"]) == 135
    for case in facts["cases"]:
        case["dialect"] = case["name"].split("_", 1)[0]
    path.write_text(json.dumps(facts, indent=2) + "\n")


if __name__ == "__main__":
    main()
