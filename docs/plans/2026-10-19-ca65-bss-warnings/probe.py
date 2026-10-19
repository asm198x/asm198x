"""Capture initialization-warning triggers without guessing from byte values."""

import importlib.util
import json
import sys
from pathlib import Path

BODIES = {
    "byte": ".byte 0\n",
    "word": ".word 0\n",
    "dword": ".dword 0\n",
    "instruction": "nop\n",
    "reserve": ".res 2\n",
    "reserve_fill_zero": ".res 2,0\n",
    "reserve_fill_nonzero": ".res 2,$AA\n",
    "reserve_empty": ".res 0\n",
    "reserve_fill_empty": ".res 0,0\n",
    "align": ".res 1\n.align 4\n",
    "align_fill_zero": ".res 1\n.align 4,0\n",
    "align_fill_empty": ".align 1,0\n",
    "empty": "",
    "string_empty": '.byte ""\n',
    "string": '.byte "A"\n',
    "incbin": '.incbin "blob.bin"\n',
    "many": ".byte 0\n.word 0\nnop\n.res 2,0\n",
    "included": '.include "data.inc"\n',
    "invalid_range": ".byte 256\n",
    "undefined": ".word MISSING\n",
    "byte_nonzero": ".byte 1\n",
    "word_nonzero": ".word 1\n",
    "dword_nonzero": ".dword 65536\n",
    "expression_zero": ".word VALUE\nVALUE=0\n",
    "expression_nonzero": ".word VALUE\nVALUE=1\n",
    "instruction_zero": "brk\n",
    "align_nonzero": ".res 1\n.align 4,$AA\n",
    "incbin_empty": '.incbin "empty.bin"\n',
    "incbin_zero": '.incbin "zero.bin"\n',
    "included_nonzero": '.include "nonzero.inc"\n',
    "reservation_around_zero": ".res 1\n.byte 0\n.res 1\n",
}

if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location(
        "capture", Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
    )
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.FILES = {
        "blob.bin": "X",
        "data.inc": ".res 1\n.byte 0\n",
        "empty.bin": "",
        "zero.bin": "\x00",
        "nonzero.inc": ".res 1\n.byte 1\n",
    }
    output = Path(sys.argv[1])
    rows, tools = [], None
    for kind in ["zp", "bss", "ro", "rw"]:
        for in_file in [False, True]:
            file = "%O" if in_file else '""'
            runner.CONFIG = f"MEMORY {{ RAM:start=$20,size=$100,file={file}; MAIN:start=$1000,size=$100,file=%O; }}\nSEGMENTS {{ ZEROPAGE:load=RAM,type={kind}; SECOND:load=RAM,type={kind}; CODE:load=MAIN,type=ro; }}\n"
            runner.cases = lambda: {
                **{
                    name: '.segment "ZEROPAGE"\n' + body + '.segment "CODE"\n.byte 9\n'
                    for name, body in BODIES.items()
                },
                "two_segments": '.segment "ZEROPAGE"\n.byte 0\n.segment "SECOND"\nnop\n.segment "ZEROPAGE"\n.word 0\n.segment "CODE"\n.byte 9\n',
            }
            original_cases = runner.cases
            runner.cases = lambda original_cases=original_cases: {
                **original_cases(),
                "two_nonzero": '.segment "ZEROPAGE"\n.byte 1\n.segment "SECOND"\nnop\n.segment "ZEROPAGE"\n.word 1\n.segment "CODE"\n.byte 9\n',
            }
            destination = output.with_name(f"{output.stem}-{kind}-{in_file}.json")
            sys.argv = [sys.argv[0], str(destination)]
            runner.main()
            facts = json.loads(destination.read_text())
            if tools is None:
                tools = facts["tools"]
            assert {k: v["sha256"] for k, v in tools.items()} == {
                k: v["sha256"] for k, v in facts["tools"].items()
            }
            for row in facts["cases"]:
                row["name"] = f"{kind}_{in_file}_{row['name']}"
                row["native_warning_count"] = sum(
                    stage["stderr"].count("contains initialized data")
                    for stage in row["native"]
                )
                rows.append(row)
    assert len(rows) == 264
    output.write_text(
        json.dumps({"tools": tools, "files": runner.FILES, "cases": rows}, indent=2)
        + "\n"
    )
    print("264 native warning controls retained")
