"""Pin file gaps separately from omitted BSS memory spans."""

import importlib.util
import json
import sys
from pathlib import Path

if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location(
        "capture", Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
    )
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    output = Path(sys.argv[1])
    rows = []
    tools = None
    for kind in ["zp", "bss", "ro", "rw"]:
        for filled in [False, True]:
            for where in ["first_start", "second_start", "second_align"]:
                extra1 = ",start=$28" if where == "first_start" else ""
                extra2 = (
                    ",start=$28"
                    if where == "second_start"
                    else ",align=$8"
                    if where == "second_align"
                    else ""
                )
                fill = ",fill=yes,fillval=$A5" if filled else ""
                runner.CONFIG = f"MEMORY {{ MAIN:start=$20,size=$20,file=%O{fill}; }}\nSEGMENTS {{ ZEROPAGE:load=MAIN,type={kind}{extra1}; CODE:load=MAIN,type=ro{extra2}; }}\n"
                runner.cases = lambda: {
                    "gap": '.segment "ZEROPAGE"\nstart:.byte 7\n.segment "CODE"\n.word start\n'
                }
                destination = output.with_name(
                    f"{output.stem}-{kind}-{filled}-{where}.json"
                )
                sys.argv = [sys.argv[0], str(destination)]
                runner.main()
                facts = json.loads(destination.read_text())
                tools = facts["tools"] if tools is None else tools
                for row in facts["cases"]:
                    row["name"] = f"{kind}_{filled}_{where}"
                    rows.append(row)
    assert len(rows) == 24
    output.write_text(
        json.dumps({"tools": tools, "files": {}, "cases": rows}, indent=2) + "\n"
    )
