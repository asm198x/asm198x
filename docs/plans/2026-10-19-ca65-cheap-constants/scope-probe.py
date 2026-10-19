"""Retain native ca65/ld65 cheap-scope anchor controls."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

CONFIG = 'MEMORY { ZP:start=$00,size=$100,file=""; MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { ZEROPAGE:load=ZP,type=zp; CODE:load=MAIN,type=ro; }\n'


def cases() -> dict[str, str]:
    return {
        "scope_empty": "anchor:\n@value=7\n.scope inner\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "scope_byte": "anchor:\n@value=7\n.scope inner\n.byte 0\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "scope_empty_string": 'anchor:\n@value=7\n.scope inner\n.byte ""\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n',
        "scope_res_zero": "anchor:\n@value=7\n.scope inner\n.res 0\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "scope_res_one": "anchor:\n@value=7\n.scope inner\n.res 1\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "scope_align_one": "anchor:\n@value=7\n.scope inner\n.align 1\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "scope_align_two": "anchor:\n@value=7\n.scope inner\n.align 2\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "scope_explicit_segment": 'anchor:\n@value=7\n.scope inner\n.segment "CODE"\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n',
        "scope_org": "anchor:\n@value=7\n.scope inner\n.org $1001\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "scope_org_align": "anchor:\n@value=7\n.scope inner\n.org $1001\n.align 2\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "scope_dead_bytes": "anchor:\n@value=7\n.scope inner\n.if 0\n.byte 0\n.endif\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "scope_repeat_zero": "anchor:\n@value=7\n.scope inner\n.repeat 0\n.byte 0\n.endrepeat\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "scope_global_constant": "anchor:\n@value=7\n.scope inner\nVALUE=9\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "scope_cheap_value": "anchor:\n@value=7\n.scope inner\n@value=9\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "scope_new_cheap": "anchor:\n@value=7\n.scope inner\n@other=9\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "scope_label_only": "anchor:\n@value=7\n.scope inner\ninside:\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "scope_nested_empty": "anchor:\n@value=7\n.scope inner\n.scope child\n.endscope\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "scope_macro_definition": "anchor:\n@value=7\n.scope inner\n.macro unused\n.byte 0\n.endmacro\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "proc_empty": "anchor:\n@value=7\n.proc inner\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "proc_byte": "anchor:\n@value=7\n.proc inner\n.byte 0\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "proc_empty_string": 'anchor:\n@value=7\n.proc inner\n.byte ""\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n',
        "proc_res_zero": "anchor:\n@value=7\n.proc inner\n.res 0\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "proc_res_one": "anchor:\n@value=7\n.proc inner\n.res 1\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "proc_align_one": "anchor:\n@value=7\n.proc inner\n.align 1\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "proc_align_two": "anchor:\n@value=7\n.proc inner\n.align 2\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "proc_explicit_segment": 'anchor:\n@value=7\n.proc inner\n.segment "CODE"\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n',
        "proc_org": "anchor:\n@value=7\n.proc inner\n.org $1001\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "proc_org_align": "anchor:\n@value=7\n.proc inner\n.org $1001\n.align 2\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "proc_dead_bytes": "anchor:\n@value=7\n.proc inner\n.if 0\n.byte 0\n.endif\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "proc_repeat_zero": "anchor:\n@value=7\n.proc inner\n.repeat 0\n.byte 0\n.endrepeat\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "proc_global_constant": "anchor:\n@value=7\n.proc inner\nVALUE=9\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "proc_cheap_value": "anchor:\n@value=7\n.proc inner\n@value=9\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "proc_new_cheap": "anchor:\n@value=7\n.proc inner\n@other=9\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "proc_label_only": "anchor:\n@value=7\n.proc inner\ninside:\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "proc_nested_empty": "anchor:\n@value=7\n.proc inner\n.scope child\n.endscope\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "proc_macro_definition": "anchor:\n@value=7\n.proc inner\n.macro unused\n.byte 0\n.endmacro\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "prior_byte": "anchor:\n@value=7\n.byte 0\n.scope inner\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "prior_res_zero": "anchor:\n@value=7\n.res 0\n.scope inner\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "prior_res_one": "anchor:\n@value=7\n.res 1\n.scope inner\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "prior_segment": 'anchor:\n@value=7\n.segment "CODE"\n.scope inner\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n',
        "prior_org": "anchor:\n@value=7\n.org $1001\n.scope inner\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "prior_align": "anchor:\n@value=7\n.align 2\n.scope inner\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "struct": "anchor:\n@value=7\n.struct Point\nfield .byte\n.endstruct\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "enum": "anchor:\n@value=7\n.enum Colors\nRED\nBLUE\n.endenum\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_0_1": "anchor:\n@value=7\n.res 0\n.scope inner\n.align 1\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_0_2": "anchor:\n@value=7\n.res 0\n.scope inner\n.align 2\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_0_3": "anchor:\n@value=7\n.res 0\n.scope inner\n.align 3\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_0_4": "anchor:\n@value=7\n.res 0\n.scope inner\n.align 4\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_0_16": "anchor:\n@value=7\n.res 0\n.scope inner\n.align 16\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_1_1": "anchor:\n@value=7\n.res 1\n.scope inner\n.align 1\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_1_2": "anchor:\n@value=7\n.res 1\n.scope inner\n.align 2\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_1_3": "anchor:\n@value=7\n.res 1\n.scope inner\n.align 3\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_1_4": "anchor:\n@value=7\n.res 1\n.scope inner\n.align 4\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_1_16": "anchor:\n@value=7\n.res 1\n.scope inner\n.align 16\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_2_1": "anchor:\n@value=7\n.res 2\n.scope inner\n.align 1\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_2_2": "anchor:\n@value=7\n.res 2\n.scope inner\n.align 2\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_2_3": "anchor:\n@value=7\n.res 2\n.scope inner\n.align 3\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_2_4": "anchor:\n@value=7\n.res 2\n.scope inner\n.align 4\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_2_16": "anchor:\n@value=7\n.res 2\n.scope inner\n.align 16\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_3_1": "anchor:\n@value=7\n.res 3\n.scope inner\n.align 1\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_3_2": "anchor:\n@value=7\n.res 3\n.scope inner\n.align 2\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_3_3": "anchor:\n@value=7\n.res 3\n.scope inner\n.align 3\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_3_4": "anchor:\n@value=7\n.res 3\n.scope inner\n.align 4\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_3_16": "anchor:\n@value=7\n.res 3\n.scope inner\n.align 16\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_4_1": "anchor:\n@value=7\n.res 4\n.scope inner\n.align 1\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_4_2": "anchor:\n@value=7\n.res 4\n.scope inner\n.align 2\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_4_3": "anchor:\n@value=7\n.res 4\n.scope inner\n.align 3\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_4_4": "anchor:\n@value=7\n.res 4\n.scope inner\n.align 4\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_4_16": "anchor:\n@value=7\n.res 4\n.scope inner\n.align 16\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_5_1": "anchor:\n@value=7\n.res 5\n.scope inner\n.align 1\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_5_2": "anchor:\n@value=7\n.res 5\n.scope inner\n.align 2\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_5_3": "anchor:\n@value=7\n.res 5\n.scope inner\n.align 3\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_5_4": "anchor:\n@value=7\n.res 5\n.scope inner\n.align 4\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "align_5_16": "anchor:\n@value=7\n.res 5\n.scope inner\n.align 16\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "width_forward": "lda VALUE\nVALUE=$12\n",
        "width_backward": "VALUE=$12\nlda VALUE\n",
        "width_zp_forward": "lda value\n.zeropage\nvalue: .res 1\n",
        "width_zp_backward": ".zeropage\nvalue: .res 1\n.code\nlda value\n",
        "width_forward_align": "anchor:\n@value=7\nlda VALUE\n.scope inner\n.align 2\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\nVALUE=$12\n",
        "scope_instruction": "anchor:\n@value=7\n.scope inner\nnop\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "scope_raw": 'anchor:\n@value=7\n.scope inner\n.incbin "data.bin"\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n',
        "scope_include": 'anchor:\n@value=7\n.scope inner\n.include "body.inc"\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n',
        "scope_repeat": "anchor:\n@value=7\n.repeat 2\n.scope\n.byte 0\n.endscope\n.endrepeat\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "scope_deferred_include": 'anchor:\n@value=7\n.repeat 1\n.scope\n.include "body.inc"\n.endscope\n.endrepeat\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n',
        "scope_macro": ".macro emit\n.scope\n.byte 0\n.endscope\n.endmacro\nanchor:\n@value=7\nemit\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "scope_macro_deferred_include": '.macro emit\n.scope\n.include "body.inc"\n.endscope\n.endmacro\nanchor:\n@value=7\nemit\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n',
        "proc_value_no_data": "anchor:\n@value=7\n.proc inner\n@value=9\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "proc_value_with_data": "anchor:\n@value=7\n.proc inner\n@value=9\n.byte 0\n.endproc\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "empty_struct": "anchor:\n@value=7\n.struct Point\n.endstruct\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "empty_union": "anchor:\n@value=7\n.union Point\n.endunion\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "empty_enum": "anchor:\n@value=7\n.enum Colors\n.endenum\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "after_scope_new_cheap": "anchor:\n@value=7\n.scope inner\n.byte 0\n.endscope\n@value=9\n.byte @value\n",
        "globalzp_forward": ".globalzp zp\nlda zp\n.zeropage\nzp: .res 1\n",
        "exportzp_forward": ".exportzp zp\nlda zp\n.zeropage\nzp: .res 1\n",
        "importzp_forward": ".importzp zp\nlda zp\n.zeropage\nzp: .res 1\n",
        "constant_shadow_earlier": "VALUE=$12\n.scope inner\nVALUE=$1234\nlda VALUE\n.endscope\n",
        "earlier_zp_forward_addend": ".zeropage\nzp: .res 1\n.code\nlda zp+1\n",
        "forward_zp_addend": "lda zp+1\n.zeropage\nzp: .res 1\n",
        "empty_segment_switch": "anchor:\n@value=7\n.scope inner\n.zeropage\n.code\n.endscope\n.byte @value\n",
        "other_segment_output": "anchor:\n@value=7\n.scope inner\n.zeropage\n.res 1\n.code\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "nested_span": "anchor:\n@value=7\n.scope outer\n.scope inner\n.byte 0\n.endscope\n.endscope\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
        "enum_with_member": "anchor:\n@value=7\n.enum Colors\nred\n.endenum\n.if .defined(@value)\n.byte @value\n.else\n.byte 255\n.endif\n",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tools = {n: Path("/opt/homebrew/bin") / n for n in ["ca65", "ld65"]}
    files = {"data.bin": "abc", "body.inc": ".byte 0\n"}
    records = []
    with tempfile.TemporaryDirectory(
        prefix="asm198x-cheap-scope-boundary-probe-"
    ) as temp:
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
