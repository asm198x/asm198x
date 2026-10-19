"""Combine native type/fill and gap controls without dropping cases."""

import json
from pathlib import Path

root = Path(__file__).parent
a = json.loads((root / "native-probes.json").read_text())
b = json.loads((root / "native-gaps.json").read_text())
assert {k: v["sha256"] for k, v in a["tools"].items()} == {
    k: v["sha256"] for k, v in b["tools"].items()
}
cases = a["cases"] + b["cases"]
assert len(cases) == 216 and len({c["name"] for c in cases}) == 216
assert sum(c["native"][-1]["exit"] == 0 for c in cases) == 152
assert all(all(s["exit"] in (0, 1) for s in c["native"]) for c in cases)
result = {"tools": a["tools"], "files": {}, "cases": cases}
text = json.dumps(result, indent=2) + "\n"
(root / "selected-native.json").write_text(text)
(root.parents[2] / "crates/asm198x/tests/fixtures/ca65-zp-output.json").write_text(text)
print("216 controls: 152 accepted, 64 refused")
