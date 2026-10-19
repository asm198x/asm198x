"""Retain native ca65/ld65 label-delimiter controls."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

CONFIG = "MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { CODE:load=MAIN,type=ro; }\n"


def cases() -> dict[str, str]:
    return {
        "attached_global": "entry:nop\n",
        "spaced_global": "entry: nop\n",
        "attached_cheap": "entry: nop\n@value:nop\n",
        "spaced_cheap": "entry: nop\n@value: nop\n",
        "attached_anonymous": ":nop\n",
        "spaced_anonymous": ": nop\n",
        "attached_byte": "entry:.byte 7\n.word entry\n",
        "attached_macro": ".macro emit value\n.byte value\n.endmacro\nentry:emit 7\n.word entry\n",
        "indented_macro": ".macro emit value\n.byte value\n.endmacro\n  entry:emit 7\n.word entry\n",
        "indented_spaced_macro": ".macro emit value\n.byte value\n.endmacro\n  entry: emit 7\n.word entry\n",
        "attached_repeat": "entry:.repeat 2\n.byte 7\n.endrepeat\n.word entry\n",
        "attached_if": "entry:.if 1\n.byte 7\n.endif\n.word entry\n",
        "attached_dead_if": "entry:.if 0\n.byte MISSING\n.endif\n.word entry\n",
        "attached_include": 'entry:.include "body.inc"\n.word entry\n',
        "attached_incbin": 'entry:.incbin "data.bin"\n.word entry\n',
        "quoted_colon": '.byte "entry:nop"\n',
        "comment_colon": ".byte 7 ; entry:nop\n",
        "scoped_operand": "VALUE=7\n.byte ::VALUE\n",
        "leading_global_equ": "::VALUE=7\n.byte VALUE\n",
        "double_colon_without_definition": "entry::nop\n",
        "invalid_numeric_label": "123:nop\n",
        "anonymous_forward": ".word :+\n:nop\n",
        "attached_macro_definition": "entry:.macro emit\n.byte 7\n.endmacro\nemit\n.word entry\n",
        "attached_text_label": ".define NAME entry\nNAME:nop\n.word entry\n",
        "cpu4510": ".p4510\nentry:ldz #7\n.word entry\n",
        "spaced_macro_definition": "entry: .macro emit\n.byte 7\n.endmacro\nemit\n.word entry\n",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tools = {n: Path("/opt/homebrew/bin") / n for n in ["ca65", "ld65"]}
    files = {"body.inc": ".byte 7\n", "data.bin": "abc"}
    records = []
    with tempfile.TemporaryDirectory(prefix="asm198x-attached-label-probe-") as temp:
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
        if r["name"] in {"attached_macro_definition", "spaced_macro_definition"}
    ]
    records = [
        r
        for r in records
        if r["name"] not in {"attached_macro_definition", "spaced_macro_definition"}
    ]
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
