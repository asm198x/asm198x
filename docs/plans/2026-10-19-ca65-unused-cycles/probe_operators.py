import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "capture", Path("docs/plans/2026-10-19-ca65-scope-binding/probe.py")
)
assert spec is not None and spec.loader is not None
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


def cases() -> dict[str, str]:
    out = {}
    for value in ["9", "$1234"]:
        for expr in [
            "RED+1",
            "RED-RED",
            "RED/0",
            "RED%0",
            "RED/(2-2)",
            "RED+MISSING",
            "RED+<MISSING",
            "RED<<-1",
            "RED<<64",
            "RED*0",
            "RED&0",
            "RED|0",
            "RED=RED",
            "RED&&0",
            "!RED",
            "<RED",
            ">RED",
            "RED+*",
            "RED+target",
        ]:
            out[f"{value}_{expr}"] = (
                f"RED={value}\ntarget:.byte 7\n.scope inner\nRED={expr}\n.endscope\n.byte 7\n"
            )
    return out


m.cases = cases
m.main()
