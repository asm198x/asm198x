"""Pin data-PC agreement between deferred value demand and actual emission."""

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
    for directive, width in [("byte", 1), ("word", 2), ("dbyt", 2), ("dword", 4)]:
        for operator in ["&&", "||"]:
            for offset in [0, width]:
                out[f"lazy_{directive}_{operator}_{offset}"] = (
                    f"RED=2\n.scope inner\nRED=RED-RED\n.org $20\n.{directive} 0,(*=${0x20 + offset:X}){operator}RED\n.endscope\n"
                )
    assert len(out) == 16
    return out


runner.cases = cases
runner.main()
