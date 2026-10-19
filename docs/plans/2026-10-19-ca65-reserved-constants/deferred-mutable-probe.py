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
        "set_positional": "V .set 1\n.byte V\nV .set 2\n.byte V\n",
        "set_instruction_width": "V .set $12\nlda V\nV .set $1234\nlda V\n",
        "set_self": "V .set 1\nV .set V+1\n.byte V\n",
        "set_alias_snapshot": "V .set 1\nALIAS=V\nV .set 2\n.byte ALIAS,V\n",
        "set_if": "V .set 0\n.if V\n.byte 7\n.else\n.byte 8\n.endif\nV .set 1\n.if V\n.byte 9\n.endif\n",
        "set_repeat": "V .set 0\n.repeat 3\nV .set V+1\n.byte V\n.endrepeat\n",
        "set_dead": "V .set 1\n.if 0\nV .set 2\n.endif\n.byte V\n",
        "equ_then_set": "V=1\nV .set 2\n.byte V\n",
        "set_then_equ": "V .set 1\nV=2\n.byte V\n",
        "set_same": "V .set 1\nV .set 1\n.byte V\n",
        "set_forward": ".byte V\nV .set 1\n",
        "set_forward_rhs": "V .set F\nF=1\n.byte V\n",
        "set_global": ".scope inner\n::V .set 1\n.byte ::V\n.endscope\n",
        "set_qualified": ".scope inner\n.endscope\ninner::V .set 1\ninner::V .set 2\n.byte inner::V\n",
        "colon_equ_width": "V:=$12\nlda V\n",
        "normal_equ_width": "V=$12\nlda V\n",
        "colon_equ_const": "V:=7\n.byte .const(V),.addrsize(V)\n",
        "normal_equ_const": "V=7\n.byte .const(V),.addrsize(V)\n",
        "set_forward_then_reassign": ".byte V\nV .set 1\n.byte V\nV .set 2\n.byte V\n",
        "set_forward_multiple": ".word V\nV .set $12\nlda V\nV .set $1234\nlda V\n",
        "set_forward_width": "lda V\nV .set $12\nlda V\nV .set $1234\nlda V\n",
        "set_scoped_shadow": "V .set 1\n.scope inner\nV .set 2\n.byte V,::V\n.endscope\n.byte V\n",
        "set_references_label": "label:\nV .set label\n.word V\n",
        "set_pc": "V .set *\n.word V\n",
        "set_late_label": "V .set label\nlabel:\n.word V\n",
        "set_byte_overflow": ".byte V\nV .set 1\nV .set 256\n",
        "set_cheap": "anchor:\n@value .set 1\n.byte @value\n@value .set 2\n.byte @value\n",
        "set_const_function": "V .set 1\n.byte .const(V)\n",
        "equ_const_function": "V=1\n.byte .const(V)\n",
        "colon_const_function": "V:=1\n.byte .const(V)\n",
        "colon_equ_reassign": "V:=1\nV:=2\n.byte V\n",
        "colon_scoped": ".scope inner\nVALUE:=7\n.byte VALUE\n.endscope\n.byte inner::VALUE\n",
        "colon_then_set": "V:=1\nV .set 2\n.byte V\n",
        "colon_forward_label": "V:=label\n.word V\nlabel:\nnop\n",
        "equ_forward_label": "V=label\n.word V\nlabel:\nnop\n",
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
    with tempfile.TemporaryDirectory(prefix="asm198x-mutable-probe-") as temp:
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
