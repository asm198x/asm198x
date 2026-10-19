"""Check that nonzero default fill is not mistaken for initialized content."""

import importlib.util
import json
import sys
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "capture", Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
)
assert spec is not None and spec.loader is not None
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
output = Path(sys.argv[1])
rows, tools = [], None
for kind in ["zp", "bss"]:
    for in_file in [False, True]:
        file = "%O" if in_file else '""'
        runner.CONFIG = f"MEMORY {{ RAM:start=$20,size=$20,file={file},fill=yes,fillval=$AA; MAIN:start=$1000,size=$20,file=%O,fill=yes,fillval=$AA; }}\nSEGMENTS {{ ZEROPAGE:load=RAM,type={kind}; CODE:load=MAIN,type=ro; }}\n"
        runner.cases = lambda: {
            "reserve": '.segment "ZEROPAGE"\n.res 2\n.segment "CODE"\n.byte 9\n',
            "align": '.segment "ZEROPAGE"\n.res 1\n.align 4\n.segment "CODE"\n.byte 9\n',
        }
        destination = output.with_name(f"{output.stem}-{kind}-{in_file}.json")
        sys.argv = [sys.argv[0], str(destination)]
        runner.main()
        facts = json.loads(destination.read_text())
        tools = tools or facts["tools"]
        for row in facts["cases"]:
            row["name"] = f"filled_{kind}_{in_file}_{row['name']}"
            row["native_warning_count"] = sum(
                s["stderr"].count("contains initialized data") for s in row["native"]
            )
            assert row["native"][-1]["exit"] == 0 and row["native_warning_count"] == 0
            rows.append(row)
assert len(rows) == 8
output.write_text(
    json.dumps({"tools": tools, "files": {}, "cases": rows}, indent=2) + "\n"
)
