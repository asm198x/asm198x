"""Select reviewed binding cases and retain every deferred case explicitly."""

import json
from pathlib import Path

ROOT = Path(__file__).parent
DEFERRED_PHASE = {
    "phase_False_VALUE_.byte .defined({x}),.const({x}),.referenced({x})",
    "phase_False_::VALUE_.byte .defined({x}),.const({x}),.referenced({x})",
    "phase_False_OTHER_.byte .defined({x}),.const({x}),.referenced({x})",
    "phase_True_VALUE_.byte .defined({x}),.const({x}),.referenced({x})",
    "phase_True_::VALUE_.byte .defined({x}),.const({x}),.referenced({x})",
    "phase_True_OTHER_.byte .defined({x}),.const({x}),.referenced({x})",
    "defined_before_ref",
    "alias_const_after",
}
DEFERRED_QUERY = {
    "root_const_ref",
    "enum_previous",
    "enum_self_parent",
    "enum_self_query",
    "cheap_label_size",
}
INCLUDED_FOLLOWUPS = {
    "record_size_cheap_constant",
    "record_size_cheap_label",
    "size_cheap_definition",
    "size_cheap_undefined",
    "size_cheap_query",
    "size_forward_label",
    "size_scoped_forward",
}


def main() -> None:
    datasets = [
        ("native-completed-phase-probes.json", 105, DEFERRED_PHASE),
        ("native-width-probes.json", 144, set()),
        ("native-negative-memory-probes.json", 28, set()),
        ("native-query-probes.json", 35, DEFERRED_QUERY),
        ("deferred-followups-native.json", 22, None),
        ("native-stream-probes.json", 6, set()),
    ]
    cases, deferred, files, tools = [], [], {}, None
    for filename, count, excluded in datasets:
        facts = json.loads((ROOT / filename).read_text())
        assert len(facts["cases"]) == count
        if tools is None:
            tools = facts["tools"]
        else:
            for name, identity in tools.items():
                assert facts["tools"][name]["sha256"] == identity["sha256"]
        for name, contents in facts.get("files", {}).items():
            assert name not in files or files[name] == contents
            files[name] = contents
        names = {case["name"] for case in facts["cases"]}
        if excluded is None:
            assert INCLUDED_FOLLOWUPS <= names
            excluded = names - INCLUDED_FOLLOWUPS
        else:
            assert excluded <= names
        for case in facts["cases"]:
            assert case["native"][-1]["exit"] != 124
            (deferred if case["name"] in excluded else cases).append(case)
    assert len(cases) == 312
    assert len({case["name"] for case in cases}) == len(cases)
    assert sum(case["native"][-1]["exit"] == 0 for case in cases) == 205
    assert len(cases) + len(deferred) == sum(count for _, count, _ in datasets)
    result = {"tools": tools, "files": files, "cases": cases}
    destination = (
        ROOT.parents[2] / "crates/asm198x/tests/fixtures/ca65-scope-binding.json"
    )
    destination.write_text(json.dumps(result, indent=2) + "\n")
    (ROOT / "deferred-cases.json").write_text(
        json.dumps({"tools": tools, "files": files, "cases": deferred}, indent=2) + "\n"
    )
    print(len(cases), "binding controls;", len(deferred), "explicit follow-ups")


if __name__ == "__main__":
    main()
