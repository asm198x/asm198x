"""Measure source function values and reference effects in lexical order."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent
FILES: dict[str, str] = {}


def cases() -> dict[str, str]:
    bodies = {
        "root_const": "VALUE=2\n.byte .ref(VALUE),.const(VALUE),.ref(VALUE)\n",
        "root_reverse": "VALUE=2\n.byte .const(VALUE),.ref(VALUE),.const(VALUE),.ref(VALUE)\n",
        "sprintf": 'VALUE=2\n.byte .ref(VALUE),.sprintf("%d",VALUE),.ref(VALUE)\n',
        "nested": 'VALUE=2\n.byte .ref(VALUE),.sprintf("%d",.const(VALUE)),.ref(VALUE)\n',
        "within_expression": "VALUE=2\n.byte .ref(VALUE)+.const(VALUE),.ref(VALUE)\n",
        "before_numeric": 'VALUE=2\n.byte VALUE,.ref(VALUE),.sprintf("%d",VALUE),.ref(VALUE)\n',
        "plain_expression": "VALUE=2\n.byte VALUE+.ref(VALUE)\n",
        "scope_outer": "VALUE=2\n.scope INNER\n.byte .defined(VALUE),.const(VALUE),.defined(VALUE),.ref(VALUE)\n.endscope\n",
        "scope_local": ".scope INNER\nVALUE=3\n.byte .ref(VALUE),.const(VALUE),.ref(VALUE)\n.endscope\n",
        "alias_condition": "L_A: .byte 0\nL_B: .byte 0\nALIAS=L_B-L_A\n.ifconst ALIAS\n.byte 7\n.else\n.byte 9\n.endif\n",
        "direct_condition": "L_A: .byte 0\nL_B: .byte 0\n.ifconst L_B-L_A\n.byte 7\n.else\n.byte 9\n.endif\n",
        "unused_const": ".macro unused_query\n.byte .const(MISSING)\n.endmacro\n.byte 7\n",
        "inactive_const": ".if 0\n.byte .const(MISSING)\n.endif\n.byte 7\n",
        "inactive_include": '.if 0\n.include .sprintf("%s",MISSING)\n.endif\n.byte 7\n',
    }
    assert len(bodies) == 14
    out = {}
    for kind, body in bodies.items():
        for context, source in {
            "direct": body,
            "included": body,
            "macro": ".macro run\n" + body + ".endmacro\nrun\n",
            "unused": ".macro unused\n" + body + ".endmacro\n.byte 7\n",
        }.items():
            name = kind + "_" + context
            if context == "included":
                filename = name + ".inc"
                FILES[filename] = source
                source = f'.include "{filename}"\n'
            out[name] = source
    assert len(out) == 56
    controls = {
        "missing_const": ".byte .const(MISSING)\n",
        "forward_const": ".byte .const(VALUE),.ref(VALUE)\nVALUE=2\n",
        "missing_sprintf": '.byte .sprintf("%d",MISSING)\n',
        "forward_sprintf": '.byte .sprintf("%d",VALUE)\nVALUE=2\n',
        "explicit_outer": "VALUE=2\n.scope INNER\n.byte .ref(::VALUE),.const(::VALUE),.ref(::VALUE)\n.endscope\n",
        "shadow_after": "VALUE=2\n.scope INNER\n.byte .defined(VALUE),.const(VALUE),.defined(VALUE),.ref(VALUE)\nVALUE=3\n.byte VALUE,.const(VALUE)\n.endscope\n",
        "cheap_const": "ROOT: .byte 0\n@VALUE=2\n.byte .ref(@VALUE),.const(@VALUE),.ref(@VALUE)\n",
        "query_only": "VALUE=2\n.byte .defined(VALUE),.ref(VALUE),.defined(VALUE),.ref(VALUE)\n",
        "numeric_before_nested": 'VALUE=2\n.byte .sprintf("%d%d",VALUE,.ref(VALUE))\n',
        "nested_before_numeric": 'VALUE=2\n.byte .sprintf("%d%d",.ref(VALUE),VALUE),.ref(VALUE)\n',
        "numeric_inside_nested": 'VALUE=2\n.byte .sprintf("%s%d",.sprintf("%d",VALUE),.ref(VALUE))\n',
        "constructor_query": 'VALUE=2\n.byte .concat(.sprintf("%d",VALUE),.sprintf("%d",.ref(VALUE)))\n',
        "label_const": "TARGET: .byte 0\n.byte .ref(TARGET),.const(TARGET),.ref(TARGET)\n",
        "forward_label_const": ".byte .ref(TARGET),.const(TARGET),.ref(TARGET)\nTARGET: .byte 0\n",
        "difference_const": "LEFT: .byte 0\nRIGHT: .byte 0\n.byte .const(RIGHT-LEFT),.ref(LEFT),.ref(RIGHT)\n",
        "alias_const": "LEFT: .byte 0\nRIGHT: .byte 0\nALIAS=RIGHT-LEFT\n.byte .ref(ALIAS),.const(ALIAS),.ref(ALIAS)\n",
        "alias_chain_const": "LEFT: .byte 0\nRIGHT: .byte 0\nFIRST=RIGHT-LEFT\nSECOND=FIRST+1\n.byte .const(SECOND)\n",
        "alias_label": "LEFT: .byte 0\nALIAS=LEFT\n.byte .const(ALIAS)\n",
        "alias_forward_difference": "FIRST=RIGHT-LEFT\n.byte .const(FIRST)\nLEFT: .byte 0\nRIGHT: .byte 0\n",
        "alias_cycle": "FIRST=SECOND\nSECOND=FIRST\n.byte .const(FIRST)\n",
        "repeat_order": "VALUE=2\n.repeat 2\n.byte .ref(VALUE),.const(VALUE),.ref(VALUE)\n.endrepeat\n",
        "repeat_scope": "VALUE=2\n.repeat 2\n.scope\n.byte .defined(VALUE),.const(VALUE),.defined(VALUE),.ref(VALUE)\n.endscope\n.endrepeat\n",
        "zero_repeat": '.repeat 0\n.byte .sprintf("%d",MISSING),.const(MISSING)\n.endrepeat\n.byte 7\n',
        "inactive_nested": '.if 0\n.byte .sprintf("%d",.const(MISSING))\n.endif\n.byte 7\n',
    }
    for kind, body in controls.items():
        for context in ["direct", "included", "macro"]:
            name = "control_" + kind + "_" + context
            if context == "included":
                filename = name + ".inc"
                FILES[filename] = body
                source = f'.include "{filename}"\n'
            elif context == "macro":
                source = ".macro run\n" + body + ".endmacro\nrun\n"
            else:
                source = body
            out[name] = source
    assert len(out) == 128
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
