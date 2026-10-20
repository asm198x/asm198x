"""Pin which numeric source reads require final symbol admission."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent
FILES: dict[str, str] = {}


def cases() -> dict[str, str]:
    bodies = {
        "missing_condition": ".ifconst MISSING\n.byte 7\n.else\n.byte 9\n.endif\n",
        "missing_cancel": ".ifconst MISSING-MISSING\n.byte 7\n.else\n.byte 9\n.endif\n",
        "missing_zero": ".ifconst MISSING*0\n.byte 7\n.else\n.byte 9\n.endif\n",
        "missing_const": ".byte .const(MISSING)\n",
        "missing_logical_const": ".byte .const(0&&MISSING)\n",
        "query_ref": ".if .ref(MISSING)\n.byte 7\n.else\n.byte 9\n.endif\n",
        "query_defined": ".if .defined(MISSING)\n.byte 7\n.else\n.byte 9\n.endif\n",
        "const_query_ref": ".ifconst .ref(MISSING)\n.byte 7\n.else\n.byte 9\n.endif\n.byte .ref(MISSING)\n",
        "const_query_defined": ".ifconst .defined(MISSING)\n.byte 7\n.else\n.byte 9\n.endif\n.byte .ref(MISSING)\n",
        "forward_condition": ".ifconst LATER\n.byte 7\n.else\n.byte 9\n.endif\n.byte .ref(LATER)\nLATER=2\n",
        "forward_cancel": ".ifconst LATER-LATER\n.byte 7\n.else\n.byte 9\n.endif\n.byte .ref(LATER)\nLATER=2\n",
        "outer_fallback": "VALUE=2\n.scope INNER\n.ifconst VALUE\n.byte 7\n.else\n.byte 9\n.endif\n.byte .defined(VALUE),.ref(VALUE)\n.endscope\n",
        "outer_shadow": "VALUE=2\n.scope INNER\n.ifconst VALUE\n.byte 7\n.else\n.byte 9\n.endif\n.byte .defined(VALUE),.ref(VALUE)\nVALUE=3\n.endscope\n",
        "explicit_missing": ".scope INNER\nVALUE=2\n.ifconst ::VALUE\n.byte 7\n.else\n.byte 9\n.endif\n.endscope\n",
        "explicit_forward": ".scope INNER\n.endscope\n.ifconst INNER::VALUE\n.byte 7\n.else\n.byte 9\n.endif\nINNER::VALUE=2\n",
        "logical_read": "VALUE=2\n.byte 0&&VALUE,.ref(VALUE)\n",
        "lazy_condition": ".if 0\n.ifconst MISSING\n.byte 7\n.endif\n.endif\n.byte 9\n",
        "lazy_macro": ".macro unused\n.ifconst MISSING\n.byte 7\n.endif\n.endmacro\n.byte 9\n",
    }
    out = {}
    for kind, body in bodies.items():
        for context in ["direct", "included", "macro"]:
            name = kind + "_" + context
            if context == "included":
                filename = name + ".inc"
                FILES[filename] = body
                source = f'.include "{filename}"\n'
            elif context == "macro":
                source = ".macro run\n" + body + ".endmacro\nrun\n"
            else:
                source = body
            out[name] = source
    assert len(out) == 54
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
