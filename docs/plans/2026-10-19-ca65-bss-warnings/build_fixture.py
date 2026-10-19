"""Retain every completed warning and default-fill control."""

import json
from pathlib import Path

root = Path(__file__).parent
a = json.loads((root / "native.json").read_text())
b = json.loads((root / "native-filled.json").read_text())
assert {k: v["sha256"] for k, v in a["tools"].items()} == {
    k: v["sha256"] for k, v in b["tools"].items()
}
a["cases"] += b["cases"]
assert len(a["cases"]) == len({c["name"] for c in a["cases"]}) == 272
assert sum(c["native"][-1]["exit"] == 0 for c in a["cases"]) == 256
assert all(all(s["exit"] in (0, 1) for s in c["native"]) for c in a["cases"])
text = json.dumps(a, indent=2) + "\n"
(root / "selected-native.json").write_text(text)
(root.parents[2] / "crates/asm198x/tests/fixtures/ca65-bss-warnings.json").write_text(
    text
)
print("272 controls: 256 accepted, 16 refused")
