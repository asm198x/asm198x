"""Measure native constancy through the existing immutable definition graph."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent
FILES: dict[str, str] = {}


def cases() -> dict[str, str]:
    bodies = {}
    expressions = {
        "difference": "RIGHT-LEFT",
        "unary": "-LEFT+RIGHT",
        "zero_left": "0*LEFT",
        "zero_right": "LEFT*0",
        "cancel": "LEFT-LEFT",
        "double": "LEFT*2",
        "sum": "LEFT+LEFT",
        "extract_label": "<LEFT",
        "scaled_difference": "(RIGHT-LEFT)*3",
        "extract_difference": ".lobyte(RIGHT-LEFT)",
        "subtraction": "0-LEFT+RIGHT",
        "divide_cancel": "LEFT/1-LEFT",
        "unit_product_left": "1*LEFT-LEFT",
        "unit_product_right": "LEFT*1-LEFT",
        "scaled_cancel": "LEFT*2-LEFT-LEFT",
        "bitnot_cancel": "~LEFT+RIGHT+1",
    }
    for kind, expression in expressions.items():
        for aliased in [False, True]:
            for condition in ["ifconst", "ifnconst"]:
                definition = f"ALIAS={expression}\n" if aliased else ""
                operand = "ALIAS" if aliased else expression
                body = (
                    "LEFT: .byte 0\nRIGHT: .byte 0\n"
                    + definition
                    + f".{condition} {operand}\n.byte 7\n.else\n.byte 9\n.endif\n"
                )
                bodies[f"{kind}_{aliased}_{condition}"] = body
    bodies.update(
        {
            "coefficient": "SCALE=2\nLEFT: .byte 0\nRIGHT: .byte 0\n.ifconst SCALE*RIGHT-SCALE*LEFT\n.byte 7\n.else\n.byte 9\n.endif\n",
            "chain": "LEFT: .byte 0\nRIGHT: .byte 0\nFIRST=RIGHT-LEFT\nSECOND=FIRST+1\n.ifconst SECOND\n.byte 7\n.else\n.byte 9\n.endif\n",
            "forward": "ALIAS=RIGHT-LEFT\n.ifconst ALIAS\n.byte 7\n.else\n.byte 9\n.endif\nLEFT: .byte 0\nRIGHT: .byte 0\n",
            "forward_cancel": "ALIAS=NEXT-NEXT\n.ifconst ALIAS\n.byte 7\n.else\n.byte 9\n.endif\nNEXT: .byte 0\n",
            "forward_then_known": "ALIAS=RIGHT-LEFT\n.ifconst ALIAS\n.byte 7\n.else\n.byte 9\n.endif\nLEFT: .byte 0\nRIGHT: .byte 0\n.ifconst ALIAS\n.byte 7\n.else\n.byte 9\n.endif\n",
            "missing": "ALIAS=MISSING-MISSING\n.ifconst ALIAS\n.byte 7\n.else\n.byte 9\n.endif\n",
            "cycle": "FIRST=SECOND\nSECOND=FIRST\n.ifconst FIRST\n.byte 7\n.else\n.byte 9\n.endif\n",
            "cross_segment": "LEFT: .byte 0\n.data\nRIGHT: .byte 0\n.code\nALIAS=RIGHT-LEFT\n.ifconst ALIAS\n.byte 7\n.else\n.byte 9\n.endif\n",
            "outer_alias": "LEFT: .byte 0\nRIGHT: .byte 0\nALIAS=RIGHT-LEFT\n.scope INNER\n.ifconst ALIAS\n.byte 7\n.else\n.byte 9\n.endif\n.endscope\n",
            "explicit_outer_alias": "LEFT: .byte 0\nRIGHT: .byte 0\nALIAS=RIGHT-LEFT\n.scope INNER\n.ifconst ::ALIAS\n.byte 7\n.else\n.byte 9\n.endif\n.endscope\n",
        }
    )
    out = {}
    for kind, body in bodies.items():
        for context in ["direct", "included", "macro", "scope"]:
            name = kind + "_" + context
            if context == "included":
                filename = name + ".inc"
                FILES[filename] = body
                source = f'.include "{filename}"\n'
            elif context == "macro":
                source = ".macro run\n" + body + ".endmacro\nrun\n"
            elif context == "scope":
                source = ".scope WRAPPER\n" + body + ".endscope\n"
            else:
                source = body
            out[name] = source
    chain = "LEFT: .byte 0\nRIGHT: .byte 0\nNAME_0=RIGHT-LEFT\n"
    chain += "".join(f"NAME_{i}=NAME_{i - 1}+1\n" for i in range(1, 301))
    out["long_chain"] = chain + ".ifconst NAME_300\n.byte 7\n.else\n.byte 9\n.endif\n"
    fan = "LEFT: .byte 0\nRIGHT: .byte 0\nNAME_0=RIGHT-LEFT\n"
    fan += "".join(f"NAME_{i}=NAME_{i - 1}+NAME_{i - 1}\n" for i in range(1, 17))
    out["shared_graph"] = fan + ".ifconst NAME_16\n.byte 7\n.else\n.byte 9\n.endif\n"
    assert len(out) == 298
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
    runner.CONFIG = (
        "MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\n"
        "SEGMENTS { CODE:load=MAIN,type=ro; DATA:load=MAIN,type=ro; }\n"
    )
    runner.cases = lambda: prepared
    runner.main()
