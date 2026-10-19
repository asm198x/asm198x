"""Retain all completed enum cases and exclude only observed native timeouts."""

import json
from pathlib import Path

root = Path(__file__).parent
first = json.loads((root / "native-probes.json").read_text())
second = json.loads((root / "native-boundaries.json").read_text())
assert {k: v["sha256"] for k, v in first["tools"].items()} == {
    k: v["sha256"] for k, v in second["tools"].items()
}
all_cases = first["cases"] + second["cases"]
completed = [case for case in all_cases if case["native"][-1]["exit"] != 124]
timeouts = [case for case in all_cases if case["native"][-1]["exit"] == 124]
assert len(all_cases) == 98 and len(completed) == 94 and len(timeouts) == 4
assert len({case["name"] for case in all_cases}) == len(all_cases)
assert sum(case["native"][-1]["exit"] == 0 for case in completed) == 39
for case in completed:
    assert all(stage["exit"] in (0, 1) for stage in case["native"])
    assert (case["native"][-1]["exit"] == 0) == (case["native_hex"] is not None)
(root / "completed-native.json").write_text(
    json.dumps({"tools": first["tools"], "files": {}, "cases": completed}, indent=2)
    + "\n"
)
print("94 completed cases; four native timeouts retained in native-boundaries.json")
