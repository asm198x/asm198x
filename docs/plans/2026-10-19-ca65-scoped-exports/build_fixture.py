"""Keep export binding distinct from tracked constancy and external gaps."""

import json
from pathlib import Path

root = Path(__file__).parent
facts = json.loads((root / "all-native.json").read_text())
deferred = json.loads((root / "deferred-cases.json").read_text())
assert len(facts["cases"]) == 274
assert len(deferred) == 52 and set(deferred.values()) == {"795", "810"}
assert sum(issue == "795" for issue in deferred.values()) == 8
assert set(deferred) <= {c["name"] for c in facts["cases"]}
cases = [c for c in facts["cases"] if c["name"] not in deferred]
assert len(cases) == len({c["name"] for c in cases}) == 222
assert sum(c["native"][-1]["exit"] == 0 for c in cases) == 135
assert all(all(s["exit"] in (0, 1) for s in c["native"]) for c in cases)
text = (
    json.dumps({"tools": facts["tools"], "files": {}, "cases": cases}, indent=2) + "\n"
)
(root / "selected-native.json").write_text(text)
(root.parents[2] / "crates/asm198x/tests/fixtures/ca65-scoped-exports.json").write_text(
    text
)
print("222 native cases: 135 accepted, 87 refused; raw follow-ups remain #795/#810")
