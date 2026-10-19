"""Account for completed value-validation controls and independent observations."""

import json
from pathlib import Path

root = Path(__file__).parent
files = [
    "native-probes.json",
    "native-operators.json",
    "native-forced-widths.json",
    "native-uses.json",
    "native-conditions.json",
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
        elif case["name"] in {
            "pc_form_ZEROPAGE",
            "segment_data_ZEROPAGE",
            "export_RED=0&&RED",
        }:
            deferred.append(case)
        else:
            assert all(stage["exit"] in (0, 1) for stage in case["native"])
            cases.append(case)
assert len(cases) == 164 and len(deferred) == 3 and len(crashes) == 4
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
    "accepted; two #801 cases, one scoped-export follow-up and four native crashes retained separately",
)

fixture = root.parents[2] / "crates/asm198x/tests/fixtures/ca65-unused-cycles.json"
fixture.write_text((root / "selected-native.json").read_text())
