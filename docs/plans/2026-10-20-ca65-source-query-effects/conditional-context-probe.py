"""Capture native conditional source reading and nested numeric text effects."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent
FILES: dict[str, str] = {}


def condition(expr: str, before: str = "VALUE=2\n", after: str = "") -> str:
    return before + f".if {expr}\n.byte 7\n.else\n.byte 9\n.endif\n" + after


def cases() -> dict[str, str]:
    tail = ".byte .ref(VALUE)\n"
    bodies = {
        "first_query": condition(".ref(VALUE)+.const(VALUE)=1", after=tail),
        "last_query": condition(".const(VALUE)+.ref(VALUE)=2", after=tail),
        "ordinary_first": condition("VALUE+.ref(VALUE)=3", after=tail),
        "ordinary_last": condition(".ref(VALUE)+VALUE=2", after=tail),
        "short_read": condition("0&&VALUE", after=tail),
        "nested_const": condition(".const(.const(VALUE))", after=tail),
        "same_segment": condition(
            ".const(RIGHT-LEFT)", "LEFT: .byte 0\nRIGHT: .byte 0\n"
        ),
        "alias_segment": condition(
            ".const(ALIAS)", "LEFT: .byte 0\nRIGHT: .byte 0\nALIAS=RIGHT-LEFT\n"
        ),
        "forward_difference": condition(
            ".const(RIGHT-LEFT)", "LEFT: .byte 0\n", "RIGHT: .byte 0\n"
        ),
        "forward_value": condition(".const(LATER)", "", ".byte .ref(LATER)\nLATER=2\n"),
        "missing": condition(".const(MISSING)", ""),
        "missing_zero": condition(".const(MISSING*0)", ""),
        "missing_logical": condition(".const(0&&MISSING)", ""),
        "query_only_ref": condition(
            ".const(.ref(MISSING))", "", ".byte .ref(MISSING)\n"
        ),
        "query_only_defined": condition(
            ".const(.defined(MISSING))", "", ".byte .ref(MISSING)\n"
        ),
        "sprintf_read": condition('.strlen(.sprintf("%d",VALUE))=1', after=tail),
        "sprintf_prefix": condition(
            '.ref(VALUE)+.strlen(.sprintf("%d",VALUE))=1', after=tail
        ),
        "sprintf_numeric_order": condition(
            '.strat(.sprintf("%d%d",.ref(VALUE),VALUE),0)=48', after=tail
        ),
        "nested_numeric_order": condition(
            '.strat(.sprintf("%d",.ref(VALUE)+.const(VALUE)),0)=49', after=tail
        ),
        "nested_string_order": condition(
            '.strlen(.sprintf("%s",.sprintf("%d",VALUE)))=1', after=tail
        ),
        "outer_fallback": condition(
            ".const(VALUE)",
            "VALUE=2\n.scope INNER\n",
            ".byte .defined(VALUE),.ref(VALUE)\n.endscope\n",
        ),
        "inner_value": condition(
            ".const(VALUE)", ".scope INNER\nVALUE=3\n", tail + ".endscope\n"
        ),
        "explicit_root": condition(
            ".const(::VALUE)",
            "VALUE=2\n.scope INNER\nVALUE=3\n",
            ".byte .ref(::VALUE),.ref(VALUE)\n.endscope\n",
        ),
        "explicit_missing": condition(
            ".const(::VALUE)", ".scope INNER\nVALUE=3\n", ".endscope\n"
        ),
        "future_shadow": condition(
            ".const(VALUE)",
            "VALUE=2\n.scope INNER\n",
            ".byte .defined(VALUE),.ref(VALUE)\nVALUE=3\n.endscope\n",
        ),
        "generated_defined": condition('.defined(.ident("VALUE"))', after=tail),
        "generated_ref": condition('.ref(.ident("VALUE"))', after=tail),
        "generated_ref_after": condition(
            '.const(VALUE)+.ref(.ident("VALUE"))=2', after=tail
        ),
        "generated_label_size": condition(
            '.sizeof(.ident("Point"))=1', "Point: .byte 1\n", ".byte .ref(Point)\n"
        ),
        "generated_size": condition(
            '.sizeof(.ident("Point"))=1', ".struct Point\nfield .byte\n.endstruct\n"
        ),
        "token_count": condition(".tcount({VALUE,.const(VALUE)})=3", after=tail),
        "token_match": condition(".xmatch({VALUE},{VALUE})", after=tail),
        "character_parenthesis": condition(".const('(')", after=tail),
        "index_const": condition('.strat("ab",.const(VALUE))=98', after=tail),
        "index_query": condition('.strat("ab",.ref(VALUE))=97', after=tail),
        "index_symbol": condition('.strat("ab",VALUE)=98', after=tail),
        "future_size": condition(
            ".const(.sizeof(FUTURE))", "", ".struct FUTURE\nfield .byte\n.endstruct\n"
        ),
        "missing_size": condition(".const(.sizeof(MISSING))", ""),
        "known_size": condition(
            ".const(.sizeof(KNOWN))", ".struct KNOWN\nfield .byte\n.endstruct\n"
        ),
        "elseif_order": "VALUE=2\n.if 0\n.byte 1\n.elseif .ref(VALUE)+.const(VALUE)=1\n.byte 7\n.else\n.byte 9\n.endif\n"
        + tail,
    }
    out = {}
    for kind, body in bodies.items():
        for context in ["direct", "included", "macro", "inactive", "unused"]:
            name = kind + "_" + context
            if context == "included":
                FILES[name + ".inc"] = body
                source = f'.include "{name}.inc"\n'
            elif context in ["macro", "unused"]:
                source = (
                    ".macro run\n"
                    + body
                    + ".endmacro\n"
                    + ("run\n" if context == "macro" else ".byte 9\n")
                )
            elif context == "inactive":
                source = ".if 0\n" + body + ".endif\n.byte 9\n"
            else:
                source = body
            out[name] = source
    assert len(out) == 200
    return out


if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location(
        "capture", ROOT.parent / "2026-10-19-ca65-scope-binding/probe.py"
    )
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    prepared = cases()
    runner.FILES = FILES
    runner.cases = lambda: prepared
    runner.main()
