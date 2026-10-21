"""Capture native source effects while reading data lists."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent
FILES: dict[str, str] = {}


def cases() -> dict[str, str]:
    bodies = {}
    for directive in [
        "byte",
        "word",
        "dbyt",
        "dword",
        "lobytes",
        "hibytes",
        "bankbytes",
        "faraddr",
    ]:
        for name, expression in {
            "list": ".ref(VALUE),.const(VALUE),.ref(VALUE)",
            "inside": ".ref(VALUE)+.const(VALUE),.ref(VALUE)",
            "ordinary": ".ref(VALUE),VALUE,.ref(VALUE)",
            "logical_missing": ".const(0&&MISSING)",
            "zero_missing": ".const(MISSING*0)",
            "forward": ".const(LATER),.ref(LATER)\nLATER=2",
            "query_only": ".const(.ref(MISSING)),.ref(MISSING)",
        }.items():
            bodies[directive + "_" + name] = f"VALUE=2\n.{directive} {expression}\n"
    bodies.update(
        {
            "sprintf_list": 'VALUE=2\n.byte .ref(VALUE),.sprintf("%d",VALUE),.ref(VALUE)\n',
            "sprintf_inside": 'VALUE=2\n.byte .ref(VALUE),.sprintf("%d%d",.ref(VALUE),VALUE),.ref(VALUE)\n',
            "concat_nested": 'VALUE=2\n.byte .ref(VALUE),.concat("x",.sprintf("%d",VALUE)),.ref(VALUE)\n',
            "strlen_inside": 'VALUE=2\n.byte .ref(VALUE)+.strlen(.sprintf("%d",VALUE)),.ref(VALUE)\n',
            "asciiz_list": 'VALUE=2\n.asciiz .sprintf("%d",.ref(VALUE)),.sprintf("%d",VALUE),.sprintf("%d",.ref(VALUE))\n',
            "current_difference": "HERE: .byte .defined(HERE),.const(*-HERE),.ref(HERE)\n",
            "current_cancel": "HERE: .byte .const(HERE-HERE),.ref(HERE)\n",
            "current_string": 'HERE: .byte .sprintf("%d",.defined(HERE)),.const(HERE-HERE)\n',
            "cheap_current": "ROOT:\n@here: .byte .const(*-@here),.ref(@here)\n",
            "scoped": "VALUE=2\n.scope INNER\n.byte .ref(VALUE),.const(VALUE),.defined(VALUE),.ref(VALUE)\n.endscope\n",
            "scoped_shadow": "VALUE=2\n.scope INNER\n.byte .ref(VALUE),.const(VALUE),.defined(VALUE),.ref(VALUE)\nVALUE=3\n.endscope\n",
            "explicit_missing": ".scope INNER\nVALUE=3\n.byte .const(::VALUE)\n.endscope\n",
            "token_count": "VALUE=2\n.byte .tcount({VALUE,.const(VALUE)}),.ref(VALUE)\n",
            "token_match": "VALUE=2\n.byte .xmatch({VALUE},{VALUE}),.ref(VALUE)\n",
            "generated_name": 'VALUE=2\n.byte .ref(.ident("VALUE")),.const(.ident("VALUE")),.ref(.ident("VALUE"))\n',
            "generated_head": 'VALUE=2\n.ident("HERE"): .byte .const(HERE-HERE),.ref(VALUE),.const(VALUE),.ref(VALUE)\n',
        }
    )
    for selector in ["left", "right", "mid"]:
        args = "0,3" if selector == "mid" else "3"
        bodies[selector + "_list"] = (
            f"VALUE=2\n.byte .ref(VALUE),.{selector}({args},{{.ref(VALUE),VALUE}}),.ref(VALUE)\n"
        )
        bodies[selector + "_bytes"] = (
            f"VALUE=2\n.byte .ref(VALUE),.{selector}({args},{{1,VALUE}}),.ref(VALUE)\n"
        )
        bodies[selector + "_words"] = (
            f"VALUE=2\n.word .ref(VALUE),.{selector}({args},{{1,VALUE}}),.ref(VALUE)\n"
        )
    for name, expr in {
        "sprintf": '.sprintf("%d",VALUE)',
        "concat": '.concat("x",.sprintf("%d",VALUE))',
        "ident": '.ident("VALUE")',
        "string": ".string(VALUE)",
        "const": ".const(VALUE)",
        "left": ".left(1,{VALUE})",
        "strlen": '.strlen(.concat("x","y"))',
        "strat": '.strat(.concat("x","y"),0)',
    }.items():
        bodies["tokens_" + name] = (
            f"VALUE=2\n.byte .ref(VALUE),.tcount({{{expr}}}),.ref(VALUE)\n"
        )
    for selector in ["left", "right", "mid"]:
        args = "0,50" if selector == "mid" else "50"
        bodies[selector + "_asciiz"] = (
            f'VALUE=2\n.asciiz .{selector}({args},{{"x",.sprintf("%d",VALUE)}})\n.byte .ref(VALUE)\n'
        )
        bodies[selector + "_query_order"] = (
            f"VALUE=2\n.byte .ref(VALUE),.{selector}({args},{{.ref(VALUE),.const(VALUE),.ref(VALUE)}}),.ref(VALUE)\n"
        )
    bodies.update(
        {
            "imported_const": ".import ext\n.byte .const(ext),.ref(ext)\n",
            "imported_zero": ".import ext\n.byte .const(ext*0)\n",
            "scoped_known": ".scope pa\nv=1\n.byte .const(v)\n.endscope\n",
            "qualified_missing": "N=1\n.byte .const(pa::N)\n",
            "selector_arithmetic": "VALUE=2\n.byte .left(1,{1})+.const(VALUE),.ref(VALUE)\n",
        }
    )
    out = {}
    for name, body in bodies.items():
        for context in ["direct", "included", "macro", "inactive", "unused"]:
            key = name + "_" + context
            if context == "included":
                FILES[key + ".inc"] = body
                source = f'.include "{key}.inc"\n'
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
            out[key] = source
    assert len(out) == 500
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
