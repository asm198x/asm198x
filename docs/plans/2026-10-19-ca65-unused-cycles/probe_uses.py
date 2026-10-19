"""Pin lazy numeric uses, export validation and source width effects."""

import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    for graph in ["RED=<RED\n", "RED=<GREEN\nGREEN=<RED\n", "RED=0&&RED\n"]:
        for use in [
            ".byte 0&&inner::RED\n",
            ".byte 1||inner::RED\n",
            ".byte 1&&inner::RED\n",
            ".byte 0||inner::RED\n",
        ]:
            out[f"lazy_{graph.strip()}_{use.strip()}"] = (
                ".scope inner\n" + graph + ".endscope\n" + use
            )
        out[f"export_{graph.strip()}"] = (
            ".scope inner\n" + graph + ".export RED\n.endscope\n.byte 7\n"
        )
        out[f"global_{graph.strip()}"] = (
            ".scope inner\n" + graph + ".global RED\n.endscope\n.byte 7\n"
        )
    for segment in ["CODE", "ZEROPAGE"]:
        out[f"pc_alias_{segment}"] = (
            f'.segment "{segment}"\nOTHER=*\n.segment "CODE"\nlda OTHER\n'
        )
        out[f"segment_data_{segment}"] = (
            f'.segment "{segment}"\n.byte 7\n.segment "CODE"\n.byte 9\n'
        )
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
