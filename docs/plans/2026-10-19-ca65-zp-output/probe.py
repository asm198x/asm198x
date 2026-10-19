"""Capture configured uninitialized-segment placement and validation."""

import importlib.util
import json
import sys
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    bodies = {
        "byte": ".byte 7\n",
        "reserve": ".res 2\n",
        "initialized_reserve": ".res 2,$AA\n",
        "invalid_byte": ".byte 256\n",
        "invalid_word": ".word 65536\n",
        "undefined_data": ".byte MISSING\n",
        "undefined_instruction": "lda MISSING\n",
        "labels": "start:.byte 7\n.word start\n",
        "pc_stride": "start:lda *\nnext:.res 1\n",
        "forced_pc_stride": "start:lda a:*\nnext:.res 1\n",
        "backward_branch": "start:.byte 7\nbne start\n",
        "empty": "",
    }
    for name, body in bodies.items():
        tail = ".word next\n" if name.endswith("stride") else ".byte 9\n"
        out[name] = '.segment "ZEROPAGE"\n' + body + '.segment "CODE"\n' + tail
    return out


if __name__ == "__main__":
    source = Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
    spec = importlib.util.spec_from_file_location("native_capture", source)
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    output = Path(sys.argv[1])
    rows, tools = [], None
    for kind in ["zp", "bss", "ro", "rw"]:
        for fill in [False, True]:
            for shared in [False, True]:
                prefix = f"{kind}_{fill}_{shared}"
                destination = output.with_name(output.stem + "-" + prefix + ".json")
                attributes = ",fill=yes,fillval=$A5" if fill else ""
                area = "ZP" if shared else "MAIN"
                runner.CONFIG = f"MEMORY {{ ZP:start=$20,size=$20,file=%O{attributes}; MAIN:start=$1000,size=$100,file=%O,fillval={'$A5' if fill else '$00'}; }}\nSEGMENTS {{ ZEROPAGE:load=ZP,type={kind}; CODE:load={area},type=ro; }}\n"
                runner.cases = cases
                sys.argv = [sys.argv[0], str(destination)]
                runner.main()
                facts = json.loads(destination.read_text())
                if tools is None:
                    tools = facts["tools"]
                assert {k: v["sha256"] for k, v in tools.items()} == {
                    k: v["sha256"] for k, v in facts["tools"].items()
                }
                for row in facts["cases"]:
                    row["name"] = prefix + "_" + row["name"]
                    rows.append(row)
    assert len(rows) == 192
    output.write_text(
        json.dumps({"tools": tools, "files": {}, "cases": rows}, indent=2) + "\n"
    )
    print("192 bounded native cases retained")
