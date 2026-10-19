"""Retain native ca65/ld65 macro-definition label controls."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

CONFIG = "MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { CODE:load=MAIN,type=ro; }\n"


def cases() -> dict[str, str]:
    return {
        "attached": "entry:.macro emit\n.byte 7\n.endmacro\nemit\n.word entry\n",
        "spaced": "entry: .macro emit\n.byte 7\n.endmacro\nemit\n.word entry\n",
        "indented": "  entry:.macro emit\n.byte 7\n.endmacro\nemit\n.word entry\n",
        "uncalled": ".byte 3\nentry:.macro emit\n.byte 7\n.endmacro\n.word entry\n",
        "call_after_bytes": ".byte 3\nentry:.macro emit\n.byte 7\n.endmacro\n.byte 4\nemit\n.word entry\n",
        "short_header": "entry:.mac emit\n.byte 7\n.endmac\nemit\n.word entry\n",
        "cheap_header": "anchor:\n@entry:.macro emit\n.byte 7\n.endmacro\nemit\n.word @entry\n",
        "anonymous_header": ":.macro emit\n.byte 7\n.endmacro\nemit\n.word :-\n",
        "local_scope": ".scope inner\nentry:.macro emit\n.byte 7\n.endmacro\n.endscope\nemit\n.word inner::entry\n",
        "selected": ".if 1\nentry:.macro emit\n.byte 7\n.endmacro\n.endif\nemit\n.word entry\n",
        "dead": ".if 0\nentry:.macro emit\n.byte MISSING\n.endmacro\n.endif\n.byte .defined(entry), .definedmacro(emit)\n",
        "captured_body": ".define V 7\nentry:.macro emit\n.byte V\n.endmacro\n.undefine V\n.define V 9\nemit\n.word entry\n",
        "body_control": "entry:.macro emit p\n.ifblank p\n.byte 7\n.else\n.byte p\n.endif\n.endmacro\nemit\n.word entry\n",
        "body_nested_calls": ".macro first\n.byte 7\n.endmacro\nentry:.macro emit\nfirst\n.endmacro\nemit\n.word entry\n",
        "definition_visible": "entry:.macro emit\n.byte 7\n.endmacro\n.byte .defined(entry), .definedmacro(emit)\nemit\n.word entry\n",
        "caller_cheap_anchor": "before:\n@value=7\nentry:.macro emit\n.byte 0\n.endmacro\n@value=9\n.byte @value\n",
        "duplicate_label": "entry: nop\nentry:.macro emit\n.byte 7\n.endmacro\nemit\n",
        "invalid_label": "123:.macro emit\n.byte 7\n.endmacro\nemit\n",
        "duplicate_macro": ".macro emit\n.endmacro\nentry:.macro emit\n.endmacro\nemit\n",
        "unclosed": "entry:.macro emit\n.byte 7\n",
        "include_definition": '.include "definition.inc"\nemit\n.word entry\n',
        "included_body": 'entry:.macro emit\n.include "body.inc"\n.endmacro\nemit\n.word entry\n',
        "comment_colon": ".macro emit ; entry:\n.byte 7\n.endmacro\nemit\n",
        "quoted_colon": 'entry:.macro emit\n.byte "entry:.macro"\n.endmacro\nemit\n.word entry\n',
        "params": "entry:.macro emit p, q\n.byte p,q\n.endmacro\nemit 7,9\n.word entry\n",
        "cpu4510": ".p4510\nentry:.macro emit\nldz #7\n.endmacro\nemit\n.word entry\n",
        "repeat_definition": ".repeat 1\nentry:.macro emit\n.byte 7\n.endmacro\n.endrepeat\nemit\n.word entry\n",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tools = {n: Path("/opt/homebrew/bin") / n for n in ["ca65", "ld65"]}
    files = {
        "body.inc": ".byte 7\n",
        "data.bin": "abc",
        "definition.inc": "entry:.macro emit\n.byte 7\n.endmacro\n",
    }
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
    args.output.write_text(
        json.dumps(
            {
                "tools": identities,
                "files": files,
                "cases": records,
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
