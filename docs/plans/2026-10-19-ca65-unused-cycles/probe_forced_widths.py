"""Check forced widths, later value uses and unused expression boundaries."""

import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    for graph in [
        "RED=<RED\n",
        "RED=>RED\n",
        "RED=0&&RED\n",
        "RED=1||RED\n",
        "RED=<GREEN\nGREEN=RED+1\n",
        "RED=<GREEN\nGREEN=<RED\n",
    ]:
        for use in [
            ".byte 7\n",
            ".word inner::RED\n",
            "OTHER=inner::RED\n.byte 7\n",
            ".export inner::RED\n.byte 7\n",
        ]:
            out[f"{graph.strip()}_{use.strip()}"] = (
                ".scope inner\n" + graph + ".endscope\n" + use
            )
    for graph in [
        "RED=MISSING\n",
        "RED=<MISSING\n",
        "RED=0&&MISSING\n",
        "RED=1||MISSING\n",
        "RED=1/0\n",
    ]:
        out[f"invalid_{graph.strip()}"] = (
            ".scope inner\n" + graph + ".endscope\n.byte 7\n"
        )
    for segment in ["CODE", "ZEROPAGE"]:
        out[f"pc_width_{segment}"] = (
            f'.segment "{segment}"\n.scope inner\nRED=RED+*\n.endscope\n.segment "CODE"\n.byte 7\n'
        )
        out[f"pc_form_{segment}"] = f'.segment "{segment}"\nlda *\n'
    return out


if __name__ == "__main__":
    source = Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
    spec = importlib.util.spec_from_file_location("native_capture", source)
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.CONFIG = "MEMORY { ZP:start=$20,size=$E0,file=%O; MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { ZEROPAGE:load=ZP,type=zp; CODE:load=MAIN,type=ro; }\n"
    runner.cases = cases
    runner.main()
