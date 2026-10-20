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
