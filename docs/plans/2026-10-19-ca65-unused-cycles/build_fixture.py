"""Account for completed value-validation controls and independent observations."""

import json
from pathlib import Path

root = Path(__file__).parent
files = [
    "native-probes.json",
    "native-operators.json",
    "native-forced-widths.json",
    "native-uses.json",
]
cases, deferred, crashes = [], [], []
tools = None
for filename in files:
    facts = json.loads((root / filename).read_text())
    if tools is None:
        tools = facts["tools"]
    assert {k: v["sha256"] for k, v in tools.items()} == {
        k: v["sha256"] for k, v in facts["tools"].items()
    }
    for case in facts["cases"]:
        if case["native"][-1]["exit"] < 0:
            crashes.append(case)
        elif case["name"] in {"pc_form_ZEROPAGE", "segment_data_ZEROPAGE"}:
            deferred.append(case)
        else:
            assert all(stage["exit"] in (0, 1) for stage in case["native"])
            cases.append(case)
assert len(cases) == 159 and len(deferred) == 2 and len(crashes) == 4
assert len({c["name"] for c in cases}) == len(cases)
for name, rows in [
    ("selected-native.json", cases),
    ("deferred-linker-cases.json", deferred),
    ("native-crashes.json", crashes),
]:
    (root / name).write_text(
        json.dumps({"tools": tools, "files": {}, "cases": rows}, indent=2) + "\n"
    )
print(
    len(cases),
    "selected controls;",
    sum(c["native"][-1]["exit"] == 0 for c in cases),
    "accepted; two #801 cases and four native crashes retained separately",
)
