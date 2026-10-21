"""Build the external contract with a complete, disjoint audit inventory."""

import json
from pathlib import Path

ROOT = Path(__file__).parent


def main() -> None:
    inputs = [
        ("external", ROOT / "native.json", 1390),
        (
            "visibility",
            ROOT.parent / "2026-10-19-ca65-scoped-exports/all-native.json",
            274,
        ),
        ("tokens", ROOT / "visibility-token-native.json", 6),
    ]
    exclusions = {
        "visibility_" + word + "_" + case: [795]
        for word in ["export", "exportzp", "global", "globalzp"]
        for case in ["queries_before_definition", "queries_with_outer"]
    }
    exclusions.update(
        {
            f"external_{word}_consumer_{case}_{context}": [795]
            for word in ["import", "importzp"]
            for case in ["if_cancel", "string_cancel"]
            for context in ["direct", "included", "macro"]
        }
    )
    audit_only = {
        "tokens_" + name: [918]
        for name in ["import_plain", "global_plain", "export_plain", "export_generated"]
    }
    assert len(exclusions) == 20 and len(audit_only) == 4
    cases = []
    files: dict[str, str] = {}
    inventory = []
    tools = None
    for prefix, path, count in inputs:
        facts = json.loads(path.read_text())
        assert len(facts["cases"]) == count > 0
        if tools is None:
            tools = facts["tools"]
        else:
            assert {name: t["sha256"] for name, t in tools.items()} == {
                name: t["sha256"] for name, t in facts["tools"].items()
            }
        for name, source in facts["files"].items():
            assert name not in files or files[name] == source, name
            files[name] = source
        for case in facts["cases"]:
            name = prefix + "_" + case["name"]
            assert name not in inventory, name
            inventory.append(name)
            if name not in exclusions and name not in audit_only:
                cases.append({**case, "name": name})
    assert len(inventory) == 1670
    names = {case["name"] for case in cases}
    assert len(names) == len(cases) == 1646
    assert names.isdisjoint(exclusions) and names.isdisjoint(audit_only)
    assert set(exclusions).isdisjoint(audit_only)
    assert names | set(exclusions) | set(audit_only) == set(inventory)
    assert sum(case["native"][-1]["exit"] == 0 for case in cases) == 1265
    output = {
        "tools": tools,
        "files": files,
        "cases": cases,
        "audit_case_count": 1670,
        "audit_names": inventory,
        "excluded_audit_cases": exclusions,
        "audit_only_cases": audit_only,
    }
    target = (
        ROOT.parents[2] / "crates/asm198x/tests/fixtures/ca65-scoped-externals.json"
    )
    target.write_text(json.dumps(output, indent=2) + "\n")
    print("qualified cases", len(cases), "audit", len(inventory))


if __name__ == "__main__":
    main()
