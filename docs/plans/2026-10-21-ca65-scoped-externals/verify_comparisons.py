"""Reject regressions and account for every external audit case."""

import json
from pathlib import Path

ROOT = Path(__file__).parent


def main() -> None:
    fixture = json.loads(
        (
            ROOT.parents[2] / "crates/asm198x/tests/fixtures/ca65-scoped-externals.json"
        ).read_text()
    )
    outstanding = set()
    for prefix, before, after, count, expected in [
        ("external", "before.json", "after.json", 1390, 12),
        ("visibility", "visibility-before.json", "visibility-after.json", 274, 8),
        (
            "tokens",
            "visibility-token-before.json",
            "visibility-token-final-after.json",
            6,
            2,
        ),
    ]:
        prior = json.loads((ROOT / before).read_text())["results"]
        current = json.loads((ROOT / after).read_text())["results"]
        assert len(prior) == len(current) == count > 0
        old = {row["name"]: row for row in prior}
        new = {row["name"]: row for row in current}
        assert len(old) == len(new) == count and old.keys() == new.keys()
        assert all(
            not row["matches_native"] or new[name]["matches_native"]
            for name, row in old.items()
        ), "previous native match lost"
        bad = [row for row in current if not row["matches_native"]]
        assert len(bad) == expected
        if prefix != "tokens":
            assert all(row["exit"] == 1 for row in bad), "wrong bytes accepted"
        outstanding.update(prefix + "_" + row["name"] for row in bad)
        print(prefix, count, "cases; remaining", len(bad), "; no lost matches")
    exclusions = set(fixture["excluded_audit_cases"])
    audit_only = set(fixture["audit_only_cases"])
    assert exclusions <= outstanding
    assert outstanding - exclusions == {"tokens_import_plain", "tokens_global_plain"}
    assert outstanding <= exclusions | audit_only
    assert len(fixture["cases"]) == 1646
    assert len(fixture["cases"]) + len(exclusions) + len(audit_only) == 1670


if __name__ == "__main__":
    main()
