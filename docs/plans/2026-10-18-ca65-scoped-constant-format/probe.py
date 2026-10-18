"""Retain native ca65/ld65 scoped immutable constant controls."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

CONFIG = "MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { CODE:load=MAIN,type=ro; }\n"


def cases() -> dict[str, str]:
    return {
        "simple": ".scope inner\nVALUE=7\n .byte VALUE\n.endscope\n",
        "unused": ".scope inner\nVALUE=7\n nop\n.endscope\n",
        "procedure": ".proc main\nVALUE=9\n lda #VALUE\n.endproc\n",
        "nested_shadow": "VALUE=1\n.scope outer\nVALUE=2\n.scope inner\nVALUE=3\n .byte VALUE,::VALUE,outer::VALUE\n.endscope\n .byte VALUE\n.endscope\n .byte VALUE\n",
        "alias": "BASE=4\n.scope inner\nVALUE=BASE+1\n .byte VALUE\n.endscope\n",
        "adjacent": ".scope inner\nV=7\nLONG_VALUE=8\n .byte V,LONG_VALUE\n.endscope\n",
        "siblings": ".scope one\nVALUE=1\n .byte VALUE\n.endscope\n.scope two\nVALUE=2\n .byte VALUE\n.endscope\n .word one::VALUE,two::VALUE\n",
        "mixed_case": ".scope Outer\nFoo=7\nbar=Foo+1\n .byte Foo,bar\n.endscope\n",
        "comments": ".scope inner ; scope\nVALUE = 7 ; value\n .byte VALUE ; bytes\n.endscope\n",
        "global_neighbors": "FIRST=6\n.scope inner\nVALUE=7\n .byte VALUE\n.endscope\nLAST=8\n .byte FIRST,inner::VALUE,LAST\n",
        "anonymous_scope": ".scope\nVALUE=7\n .byte VALUE\n.endscope\n",
        "repeat": ".repeat 2,I\n.scope\nVALUE=I\n .byte VALUE\n.endscope\n.endrepeat\n",
        "macro": ".macro emit_value value\n.scope\nVALUE=value\n .byte VALUE\n.endscope\n.endmacro\n emit_value 5\n emit_value 6\n",
        "live_branch": ".if 1\n.scope inner\nVALUE=7\n .byte VALUE\n.endscope\n.endif\n",
        "dead_branch": ".if 0\n.scope inner\nVALUE=7\n .byte VALUE\n.endscope\n.endif\n nop\n",
        "include_debug": 'before: nop\n.scope inner\n.include "constants.inc"\n .byte VALUE\n.endscope\nafter: nop\n.word before,after\n',
        "include_wrapped": '.include "wrapped.inc"\n .byte inner::VALUE\n',
        "qualified_inside": ".scope inner\ninner::VALUE=7\n nop\n.endscope\n",
        "qualified_outside": ".scope inner\n.endscope\ninner::VALUE=7\n.byte inner::VALUE\n",
        "qualified_global": ".scope inner\n::VALUE=7\n.byte ::VALUE\n.endscope\n",
        "qualified_nested": ".scope outer\n.scope inner\n.endscope\n.endscope\nouter::inner::VALUE=7\n.byte outer::inner::VALUE\n",
        "qualified_relative": ".scope outer\n.scope inner\n.endscope\ninner::VALUE=7\n.byte inner::VALUE\n.endscope\n",
        "qualified_global_nested": ".scope outer\n.scope inner\n.endscope\n.endscope\n::outer::inner::VALUE=7\n.byte ::outer::inner::VALUE\n",
        "missing_nested_refusal": ".scope outer\n.endscope\nouter::missing::VALUE=7\nnop\n",
        "missing_global_nested_refusal": ".scope outer\n.endscope\n::outer::missing::VALUE=7\nnop\n",
        "empty_segment_refusal": ".scope outer\n.endscope\nouter::::VALUE=7\nnop\n",
        "duplicate_changed_refusal": "VALUE=7\nVALUE=8\n.byte VALUE\n",
        "duplicate_same_refusal": "VALUE=7\nVALUE=7\n.byte VALUE\n",
        "duplicate_qualified_refusal": ".scope inner\nVALUE=7\ninner::VALUE=8\n.byte VALUE\n.endscope\n",
        "duplicate_unresolved_refusal": "VALUE=FORWARD\nVALUE=8\nFORWARD=7\n.byte VALUE\n",
        "label_then_equ_refusal": "VALUE: nop\nVALUE=7\n.byte VALUE\n",
        "equ_then_label_refusal": "VALUE=7\nVALUE: nop\n.byte VALUE\n",
        "alternative_branches": ".if 1\nVALUE=7\n.else\nVALUE=8\n.endif\n.byte VALUE\n",
        "alternative_branches_false": ".if 0\nVALUE=7\n.else\nVALUE=8\n.endif\n.byte VALUE\n",
        "dead_duplicate": "VALUE=7\n.if 0\nVALUE=8\n.endif\n.byte VALUE\n",
        "duplicate_include_refusal": '.scope inner\n.include "constants.inc"\n.include "constants.inc"\n.byte VALUE\n.endscope\n',
        "duplicate_repeat_refusal": ".repeat 2\nVALUE=7\n.endrepeat\n.byte VALUE\n",
        "global_qualified_refusal": "inner::VALUE=7\n nop\n",
        "unnamed_siblings": ".scope\nVALUE=7\n.byte VALUE\n.endscope\n.scope\nVALUE=8\n.byte VALUE\n.endscope\n",
        "unnamed_forward_label": ".scope\n.word target\ntarget: nop\n.endscope\n",
        "nested_procedures": ".proc outer\nVALUE=7\n.proc inner\nVALUE=8\n.byte VALUE\n.endproc\n.byte VALUE\n.endproc\n.word outer,outer::inner\n",
        "unnamed_procedure": ".proc\nVALUE=7\n.byte VALUE\n.endproc\n",
        "repeat_nested_scopes": ".repeat 2,I\n.scope\n.scope inner\nVALUE=I+1\n.byte VALUE\n.endscope\n.endscope\n.endrepeat\n",
        "repeat_shadow": ".repeat 2,I\n.scope\nI=7\nVALUE=I\n.byte VALUE\n.endscope\n.endrepeat\n",
        "zero_repeat_duplicate": "VALUE=7\n.repeat 0\nVALUE=8\n.endrepeat\n.byte VALUE\n",
        "include_qualified_definition": '.include "wrapped.inc"\ninner::OTHER=8\n.byte inner::OTHER\n',
        "qualified_shadow_parent_refusal": ".scope head\n.scope leaf\n.endscope\n.endscope\n.scope outer\n.scope head\n.endscope\nhead::leaf::VALUE=7\n.endscope\n",
        "empty_global_name_refusal": "::=7\nnop\n",
        "invalid_name_refusal": "1VALUE=7\nnop\n",
        "global_then_scoped_duplicate_refusal": "VALUE=7\n.scope inner\n::VALUE=8\n.endscope\n",
        "unclosed_refusal": ".scope inner\nVALUE=7\n nop\n",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tools = {n: Path("/opt/homebrew/bin") / n for n in ["ca65", "ld65"]}
    files = {
        "constants.inc": "VALUE=7\n",
        "wrapped.inc": ".scope inner\nVALUE=7\n nop\n.endscope\n",
    }
    records = []
    with tempfile.TemporaryDirectory(prefix="asm198x-scoped-constants-probe-") as temp:
        root = Path(temp)
        for name, contents in files.items():
            (root / name).write_text(contents)

        def run(argv: list[str]) -> dict:
            r = subprocess.run(
                argv, cwd=root, capture_output=True, timeout=10, check=False
            )
            return {
                "argv": argv,
                "cwd": str(root),
                "exit": r.returncode,
                "stdout": r.stdout.decode(errors="replace"),
                "stderr": r.stderr.decode(errors="replace"),
            }

        identities = {
            n: {
                "path": str(p),
                "sha256": hashlib.sha256(p.read_bytes()).hexdigest(),
                "version": run([str(p), "--version"]),
            }
            for n, p in tools.items()
        }
        (root / "input.cfg").write_text(CONFIG)
        for name, source in cases().items():
            (root / "input.s").write_text(source)
            for file in ["input.o", "native.bin"]:
                (root / file).unlink(missing_ok=True)
            stages = [run([str(tools["ca65"]), "input.s", "-o", "input.o"])]
            if stages[-1]["exit"] == 0:
                stages.append(
                    run(
                        [
                            str(tools["ld65"]),
                            "-C",
                            "input.cfg",
                            "input.o",
                            "-o",
                            "native.bin",
                        ]
                    )
                )
            records.append(
                {
                    "name": name,
                    "source": source,
                    "config": CONFIG,
                    "native": stages,
                    "native_hex": (root / "native.bin").read_bytes().hex()
                    if (root / "native.bin").exists()
                    else None,
                }
            )
    deferred = [
        r for r in records if r["name"] in {"unnamed_procedure", "repeat_shadow"}
    ]
    records = [r for r in records if r not in deferred]
    (Path(__file__).parent / "deferred-native.json").write_text(
        json.dumps({"tools": identities, "files": files, "cases": deferred}, indent=2)
        + "\n"
    )
    args.output.write_text(
        json.dumps({"tools": identities, "files": files, "cases": records}, indent=2)
        + "\n"
    )

    print(
        "cases",
        len(records),
        "accepted",
        sum(r["native"][-1]["exit"] == 0 for r in records),
    )


if __name__ == "__main__":
    main()
