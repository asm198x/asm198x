"""Retain each operand's own data PC and the corresponding range boundaries."""

import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "capture", Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
)
assert spec is not None and spec.loader is not None
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
runner.CONFIG = 'MEMORY { MAIN:start=$1000,size=$7000,file=%O; RAM:start=$20,size=$100,file=""; }\nSEGMENTS { CODE:load=MAIN,type=ro; ZEROPAGE:load=RAM,type=zp; }\n'


def cases() -> dict[str, str]:
    out = {}
    for directive in ["byte", "word", "dbyt", "dword"]:
        for origin in ["$20", "$FE", "$FF", "$FFFE", "$FFFFFFFC"]:
            out[f"data_{directive}_{origin}"] = f".org {origin}\n.{directive} *,*,*\n"
    out.update(
        {
            "data_reloc_bytes": ".byte <*,<*,<*\n",
            "data_reloc_words": ".word *,*,*\n",
            "data_strings": '.org $20\n.byte "A",*,"BC",*\n',
            "data_operators": ".org $20\n.byte <*,>*,<*\n",
            "data_alias_pc": ".org $20\nPC_ALIAS=*\n.word PC_ALIAS,*\n",
            "data_forward_pc": ".org $20\n.word ALIAS,*\nALIAS=*\n",
            "data_instruction_baseline": ".org $2000\nlda *\n.word *\n",
        }
    )
    assert len(out) == 27
    return out


runner.cases = cases
runner.main()
