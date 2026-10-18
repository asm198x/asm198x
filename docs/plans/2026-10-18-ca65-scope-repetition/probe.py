"""Retain native ca65/ld65 procedure and repeat-token controls."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

CONFIG = "MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { CODE:load=MAIN,type=ro; }\n"


def cases() -> dict[str, str]:
    return {
        "unnamed_proc_scope_close": ".proc\nVALUE=7\n.byte VALUE\n.endscope\n",
        "unnamed_proc_proc_close": ".proc\nVALUE=7\n.byte VALUE\n.endproc\n",
        "named_proc_proc_close": ".proc main\nVALUE=7\n.byte VALUE\n.endproc\n.word main\n",
        "named_proc_scope_close": ".proc main\n.byte 7\n.endscope\n",
        "unnamed_scope_proc_close": ".scope\n.byte 7\n.endproc\n",
        "unnamed_proc_nested": ".proc outer\n.proc\nVALUE=7\n.byte VALUE\n.endscope\n.endproc\n.word outer\n",
        "unnamed_proc_repeat": ".repeat 2,I\n.proc\nVALUE=I\n.byte VALUE\n.endscope\n.endrepeat\n",
        "repeat_define_counter": ".repeat 2,I\n.scope\nI=7\n.byte I\n.endscope\n.endrepeat\n",
        "repeat_define_other": ".repeat 2,I\n.scope\nVALUE=I\n.byte VALUE\n.endscope\n.endrepeat\n",
        "repeat_label_counter": ".repeat 1,I\nI: nop\n.endrepeat\n",
        "repeat_scope_counter": ".repeat 1,I\n.scope I\n.byte I\n.endscope\n.endrepeat\n",
        "repeat_proc_counter": ".repeat 1,I\n.proc I\n.byte I\n.endproc\n.endrepeat\n",
        "repeat_qualified_counter": ".scope inner\n.endscope\n.repeat 1,I\ninner::I=7\n.endrepeat\n.byte inner::I\n",
        "repeat_prefix_counter": ".repeat 2,I\n.scope\nIVALUE=7\n.byte IVALUE,I\n.endscope\n.endrepeat\n",
        "repeat_quoted_counter": '.repeat 2,I\n.byte "I",I ; I\n.endrepeat\n',
        "repeat_nested_counter": ".repeat 2,I\n.repeat 2,J\n.byte I,J\n.endrepeat\n.endrepeat\n",
        "repeat_nested_shadow": ".repeat 2,I\n.repeat 2,I\n.byte I\n.endrepeat\n.byte I\n.endrepeat\n",
        "repeat_zero_invalid": ".repeat 0,I\nI=7\n.endrepeat\nnop\n",
        "repeat_cheap_local": ".repeat 1,I\n.scope\nmain: nop\n@I: nop\nbne @I\n.byte I\n.endscope\n.endrepeat\n",
        "repeat_hex_token": ".repeat 2,B\n.byte $B,B\n.endrepeat\n",
        "repeat_bad_hex": ".repeat 2,I\n.byte $I\n.endrepeat\n",
        "repeat_bad_binary": ".repeat 2,I\n.byte %I\n.endrepeat\n",
        "repeat_word_boundaries": ".repeat 2,I\n.scope\n_I=7\nI_=8\n.byte _I,I_,I\n.endscope\n.endrepeat\n",
        "repeat_unicode_string": '.repeat 2,I\n.byte "éI",I\n.endrepeat\n',
        "repeat_unicode_comment": ".repeat 2,I\n.byte I ; éI\n.endrepeat\n",
        "repeat_define_text": ".repeat 2,I\n.define V I\n.byte V\n.undefine V\n.endrepeat\n",
        "repeat_conditional": ".repeat 2,I\n.if I\n.byte 7\n.else\n.byte 8\n.endif\n.endrepeat\n",
        "repeat_dead_define": ".repeat 2,I\n.if 0\nI=7\n.endif\n.byte I\n.endrepeat\n",
        "repeat_include": '.repeat 2,I\n.include "counter.inc"\n.endrepeat\n',
        "repeat_case_sensitive": ".repeat 2,I\n.scope\ni=7\n.byte I,i\n.endscope\n.endrepeat\n",
        "repeat_empty_name": ".repeat 2,\n.byte 7\n.endrepeat\n",
        "repeat_bad_name": ".repeat 2,1\n.byte 7\n.endrepeat\n",
        "macro_unicode_string": '.macro emit value\n.byte "é",value\n.endmacro\nemit 7\n',
        "repeat_outer_define": ".define V I\n.repeat 2,I\n.byte V\n.endrepeat\n",
        "repeat_include_scoped_constant": 'before: nop\n.repeat 2,I\n.scope\n.include "constants.inc"\n.byte VALUE\n.endscope\n.endrepeat\nafter: nop\n.word before,after\n',
        "include_debug": 'before: nop\n.include "constants.inc"\n.byte VALUE\nafter: nop\n.word before,after\n',
        "variable_set": "VALUE .set 7\nVALUE .set 8\n.byte VALUE\n",
        "variable_set_scope": ".scope inner\nVALUE .set 7\nVALUE .set 8\n.byte VALUE\n.endscope\n",
        "colon_equ": "VALUE :=7\n.byte VALUE\n",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tools = {n: Path("/opt/homebrew/bin") / n for n in ["ca65", "ld65"]}
    files = {
        "constants.inc": "VALUE=7\n",
        "counter.inc": ".byte I\n",
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
        r
        for r in records
        if r["name"]
        in {
            "variable_set",
            "variable_set_scope",
            "colon_equ",
            "repeat_include_scoped_constant",
        }
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
