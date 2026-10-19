"""Retain native ca65/ld65 cheap-local constant controls."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

CONFIG = "MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { CODE:load=MAIN,type=ro; }\n"


def cases() -> dict[str, str]:
    return {
        "cheap_basic": "anchor:\n@value=7\n.byte @value\n",
        "cheap_no_anchor": "@value=7\n.byte @value\n",
        "cheap_two_anchors": "first:\n@value=7\n.byte @value\nsecond:\n@value=9\n.byte @value\n",
        "cheap_duplicate": "anchor:\n@value=7\n@value=9\n.byte @value\n",
        "cheap_const_boundary": "anchor:\n@value=7\nVALUE=9\n.byte @value\n",
        "cheap_keeps_anchor": "anchor:\n@label: nop\n@value=7\n.word @label\n.byte @value\n",
        "cheap_forward": "anchor:\n.byte @value\n@value=7\n",
        "cheap_forward_rhs": "anchor:\n@value=VALUE\nVALUE=7\n.byte @value\n",
        "cheap_inner_scope": "anchor:\n@value=7\n.scope inner\n@value=9\n.byte @value\n.endscope\n.byte @value\n",
        "cheap_proc": ".proc first\n@value=7\n.byte @value\n.endproc\n.proc second\n@value=9\n.byte @value\n.endproc\n",
        "cheap_global_qualified": "::@value=7\n.byte ::@value\n",
        "cheap_digit": "anchor:\n@1value=7\n.byte @1value\n",
        "cheap_dotted": "anchor:\n@va.lue=7\n.byte @va.lue\n",
        "cheap_defined": "anchor:\n@value=7\n.if .defined(@value)\n.byte @value\n.endif\n",
        "cheap_macro": ".macro emit value\n.local anchor\nanchor:\n@value=value\n.byte @value\n.endmacro\nemit 7\nemit 9\n",
        "cheap_rhs_global_boundary": "anchor:\n@value=7\nVALUE=@value\n.byte VALUE\n",
        "cheap_read_scope": "anchor:\n@value=7\n.scope inner\n.byte @value\n.endscope\n.byte @value\n",
        "cheap_new_global_in_scope": "anchor:\n@value=7\n.scope inner\nnext:\n@value=9\n.byte @value\n.endscope\n.byte @value\n",
        "cheap_after_proc": "anchor:\n@value=7\n.proc inner\n@value=9\n.byte @value\n.endproc\n.byte @value\n",
        "cheap_proc_outer_reference": "anchor:\n@value=7\n.proc inner\n.byte @value\n.endproc\n",
        "cheap_defined_expr": "anchor:\n@value=7\n.byte .defined(@value)\n",
        "cheap_arithmetic": "anchor:\n@value=7\n.byte @value+1\n",
        "cheap_digit_arithmetic": "anchor:\n@1value=7\n.byte @1value+1\n",
        "cheap_forward_digit": "anchor:\n.byte @1value\n@1value=7\n",
        "cheap_const_first_anchor": "VALUE=9\n@value=7\n.byte @value\n",
        "cheap_qualified_const_boundary": ".scope inner\n.endscope\nanchor:\n@value=7\ninner::VALUE=9\n.byte @value\n",
        "cheap_dot_dead": ".if 0\n@value=7\n.endif\n.byte 7\n",
        "cheap_include": 'anchor:\n.include "values.inc"\n.byte @value\n',
        "cheap_repeat": ".repeat 2\n.scope\nanchor:\n@value=7\n.byte @value\n.endscope\n.endrepeat\n",
        "cheap_global_name_collision": "anchor:\n@value=7\nvalue=9\n.byte value\n",
        "cheap_only_inside_scope": "anchor:\n@value=7\n.scope inner\n.byte @value\n.endscope\n",
        "cheap_define_after_scope": "anchor:\n.scope inner\n.endscope\n@value=7\n.byte @value\n",
        "cheap_define_after_proc": "anchor:\n.proc inner\n.endproc\n@value=7\n.byte @value\n",
        "cheap_unused_scope": "anchor:\n@value=7\n.scope inner\n.endscope\n.byte @value\n",
        "cheap_scope_no_global": ".scope inner\n@value=7\n.byte @value\n.endscope\n",
        "cheap_proc_empty_return": "anchor:\n@value=7\n.proc inner\n.endproc\n.byte @value\n",
        "cheap_ifdef": "anchor:\n@value=7\n.ifdef @value\n.byte @value\n.endif\n",
        "cheap_if": "anchor:\n@value=7\n.if @value\n.byte @value\n.endif\n",
        "cheap_ifconst": "anchor:\n@value=7\n.ifconst @value\n.byte @value\n.endif\n",
        "cheap_repeat_count": "anchor:\n@count=2\n.repeat @count\n.byte 7\n.endrepeat\n",
        "cheap_macro_external": ".macro emit value\n@value=value\n.endmacro\nanchor:\nemit 7\n.byte @value\n",
        "cheap_macro_label_external": ".macro emit value\n@value: .byte value\n.endmacro\nanchor:\nemit 7\n.word @value\n",
        "cheap_ifref": "anchor:\n@value=7\n.byte @value\n.ifref @value\n.byte 9\n.endif\n",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tools = {n: Path("/opt/homebrew/bin") / n for n in ["ca65", "ld65"]}
    files = {"values.inc": "@value=7\n"}
    records = []
    with tempfile.TemporaryDirectory(prefix="asm198x-cheap-constants-probe-") as temp:
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
    deferred_names = {
        "cheap_inner_scope",
        "cheap_read_scope",
        "cheap_new_global_in_scope",
        "cheap_after_proc",
        "cheap_only_inside_scope",
        "cheap_scope_no_global",
        "cheap_proc_empty_return",
    }
    deferred = [r for r in records if r["name"] in deferred_names]
    records = [r for r in records if r["name"] not in deferred_names]
    args.output.write_text(
        json.dumps(
            {
                "tools": identities,
                "files": files,
                "cases": records,
                "deferred_cases": deferred,
            },
            indent=2,
        )
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
