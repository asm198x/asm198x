"""Native constructor, generated punctuation and unbraced-list boundaries."""

import importlib.util
import json
import sys
from pathlib import Path
from types import ModuleType

ROOT = Path(__file__).resolve().parents[4]


def load(name: str, path: Path) -> ModuleType:
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


empty = load("empty", ROOT / "docs/plans/2026-10-21-ca65-empty-calls/probe.py")
BODIES = {
    "apostrophe_count": ".byte .tcount({'''})",
    "quoted_character_kind": ".byte .xmatch({'''},{'\"'})",
    "quoted_string_value": '.byte .xmatch({"\'"},{""})',
    "selected_open": ".byte .tcount({.left(1,{{1})})",
    "selected_two_open": ".byte .tcount({.left(2,{{{1})})",
    "selected_close": ".byte .tcount({.left(1,})})",
    "selected_bare_close": ".byte .tcount(.left(1,}))",
    "bare_left_string": '.byte .left(1,"AB")',
    "bare_left_number": ".byte .left(1,7)",
    "bare_left_name": ".byte .ref(VALUE),.left(1,VALUE),.ref(VALUE)",
    "bare_left_fields": ".byte .left(3,1,2)",
    "bare_count_expr": ".byte .tcount(1+2)",
    "bare_count_fields": ".byte .tcount(1,2)",
    "bare_count_nested": ".byte .tcount(.max(1,2))",
    "bare_match_names": ".byte .match(VALUE,OTHER)",
    "bare_match_numbers": ".byte .match(1,2)",
    "selector_count_query": ".res .left(.const(VALUE)+2,{1,2})+1",
    "selector_count_selector": ".res .left(.left(1,{3}),{1,2})+1",
    "string_register": ".byte .string(A)",
    "string_query": ".byte .string(.const(VALUE))",
    "string_number_selector": ".byte .string(.left(1,{7}))",
    "ident_selected_register": '.byte .tcount({.left(1,{.ident("A")})})',
    "ident_register_kind": '.byte .xmatch({.left(1,{.ident("A")})},{A})',
    "sprintf_query": '.byte .sprintf("%d",.const(VALUE))',
    "concat_numeric": '.byte .concat("A",.const(VALUE))',
    "selected_open_as_match": ".byte .match({.left(1,{{1})},{{})",
    "selected_comma_count": ".byte .tcount({.left(3,{1,2})})",
    "string_expr_number": ".byte .string(1+2)",
    "string_expr_name": ".byte .string(VALUE+1)",
    "string_negative": ".byte .string(-1)",
    "string_query_add": ".byte .string(.const(VALUE)+1)",
    "string_ref": ".byte .ref(VALUE),.string(.ref(VALUE)),.ref(VALUE)",
    "string_char": ".byte .string('A')",
    "string_generated_register": '.byte .string(.ident("A"))',
    "string_selected_register": ".byte .string(.left(1,{A}))",
    "string_selected_ident": '.byte .string(.left(1,{.ident("A")}))',
}


def cases() -> dict[str, str]:
    out = {}
    for name, body in BODIES.items():
        empty.add_contexts(out, "ca65", name, "VALUE=7\nOTHER=8\n" + body + "\n")
    assert len(out) == 180
    return out


def main() -> None:
    capture = load(
        "capture", ROOT / "docs/plans/2026-10-19-ca65-scope-binding/probe.py"
    )
    prepared = cases()
    capture.cases = lambda: prepared
    capture.FILES = empty.FILES
    capture.main()
    path = Path(sys.argv[1])
    facts = json.loads(path.read_text())
    assert len(facts["cases"]) == 180
    for case in facts["cases"]:
        case["dialect"] = "ca65"
    path.write_text(json.dumps(facts, indent=2) + "\n")


if __name__ == "__main__":
    main()
