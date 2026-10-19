"""Arbitrate the existing .org unit's statement-PC expectation with native."""

import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "capture", Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
)
assert spec is not None and spec.loader is not None
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
runner.CONFIG = "MEMORY { MAIN:start=$8000,size=$8000,file=%O; }\nSEGMENTS { CODE:load=MAIN,type=ro; }\n"
runner.cases = lambda: {
    "existing_org_unit": '.segment "CODE"\n .byte $11\n.org $2000\nL: .byte $22\n .word L, *\n.reloc\nM: .byte $33\n .word M\n'
}
runner.main()
