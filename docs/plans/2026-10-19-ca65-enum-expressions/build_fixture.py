"""Combine every completed enum control and retain native timeouts separately."""

import json
from pathlib import Path

root = Path(__file__).parent
first = json.loads((root / "completed-native.json").read_text())
extra = json.loads((root / "native-extensions.json").read_text())
assert {k: v["sha256"] for k, v in first["tools"].items()} == {
    k: v["sha256"] for k, v in extra["tools"].items()
}
cases = first["cases"] + extra["cases"]
assert len(cases) == 136 and len({case["name"] for case in cases}) == 136
assert sum(case["native"][-1]["exit"] == 0 for case in cases) == 64
assert all(all(stage["exit"] in (0, 1) for stage in case["native"]) for case in cases)
files = {**first.get("files", {}), **extra.get("files", {})}
result = {"tools": first["tools"], "files": files, "cases": cases}
(root / "selected-native.json").write_text(json.dumps(result, indent=2) + "\n")
print(
    "136 completed controls: 64 accepted, 72 refused; four native timeouts retained separately"
)

fixture = root.parents[2] / "crates/asm198x/tests/fixtures/ca65-enum-expressions.json"
fixture.write_text((root / "selected-native.json").read_text())
