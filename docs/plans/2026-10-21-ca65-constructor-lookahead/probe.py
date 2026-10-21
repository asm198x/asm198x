"""Observe native lookahead at name and selector consumption boundaries."""

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
    "adjacent_empty": "VALUE=2\n.byte VALUE .left(.ref(VALUE),{+1})\n",
    "adjacent_incomplete": "VALUE=2\n.byte VALUE .left(.ref(VALUE)+1,{+1})\n",
    "physical_operator": "VALUE=2\n.byte VALUE+.left(.ref(VALUE),{1})\n",
    "namespace_separator": ".scope OUTER\nINNER=7\n.endscope\n.byte OUTER .left(1,{::}) INNER\n",
    "selected_name": "VALUE=2\n.byte .left(1,{VALUE}) .left(.ref(VALUE),{+1})\n",
    "selected_fields": "VALUE=2\n.byte .left(3,{VALUE,0}) .left(.ref(VALUE),{+1})\n",
    "selected_physical_operator": "VALUE=2\n.byte .left(3,{VALUE,0}) + .left(.ref(VALUE),{1})\n",
}


def cases() -> dict[str, str]:
    out: dict[str, str] = {}
    for name, body in BODIES.items():
        empty.add_contexts(out, "ca65", name, body)
    assert len(out) == 35
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
    assert len(facts["cases"]) == 35
    for case in facts["cases"]:
        case["dialect"] = "ca65"
    path.write_text(json.dumps(facts, indent=2) + "\n")


if __name__ == "__main__":
    main()
