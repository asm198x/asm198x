"""Retain native ca65/ld65 lexical reference phase controls."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

CONFIG = 'MEMORY { ZP:start=$20,size=$E0,file=""; MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { ZEROPAGE:load=ZP,type=zp,optional=yes; CODE:load=MAIN,type=ro; }\n'


def cases() -> dict[str, str]:
    out = {}
    for kind, definition in [("constant", "VALUE=$34"), ("label", "VALUE: .byte 7")]:
        for expr in ["VALUE", "VALUE+1", "<VALUE", ">VALUE", "VALUE*2"]:
            for use in [
                ".word OTHER",
                "lda OTHER",
                ".byte .defined(OTHER)",
                ".res OTHER",
            ]:
                name = f"forward_{kind}_{expr}_{use}"
                out[name] = f"OTHER={expr}\n{use}\n{definition}\n"
    for name, source in {
        "unused_undefined": "OTHER=MISSING\n.byte 7\n",
        "unused_cycle": "FIRST=SECOND\nSECOND=FIRST\n.byte 7\n",
        "unused_self": "OTHER=OTHER\n.byte 7\n",
        "alias_chain": "FIRST=SECOND\nSECOND=VALUE\n.word FIRST\nVALUE=$1234\n",
        "alias_updates": "OTHER=VALUE\nVALUE=2\n.res OTHER\n",
        "chain_updates": "FIRST=SECOND\nSECOND=VALUE\nVALUE=2\n.res FIRST\n",
        "zero_page_alias": '.segment "ZEROPAGE"\nOTHER=target\ntarget: .res 1\n.segment "CODE"\nlda OTHER\n',
        "pc_early": "OTHER=*\n.word OTHER\n",
        "pc_after_byte": ".byte 7\nOTHER=*\n.word OTHER\n",
        "pc_before_org": "OTHER=*\n.org $2000\n.word OTHER\n",
        "pc_after_org": ".org $2000\nOTHER=*\n.word OTHER\n",
        "pc_alias_chain": "FIRST=SECOND\nSECOND=*\n.byte 7\n.word FIRST\n",
        "pc_unused": "OTHER=*\n.byte 7\n",
        "scoped_forward": ".scope inner\nOTHER=VALUE\n.word OTHER\nVALUE=$34\n.endscope\n",
        "scoped_updates": ".scope inner\nOTHER=VALUE\nVALUE=2\n.res OTHER\n.endscope\n",
        "cheap_forward": "start:\n@other=@value\n.word @other\n@value: .byte 7\n",
        "export_alias": ".export OTHER\nOTHER=target\n.word OTHER\ntarget: .byte 7\n",
        "import_alias": ".import OTHER\nOTHER=target\ntarget: .byte 7\n",
        "duplicate_alias": "OTHER=VALUE\nOTHER=2\nVALUE=3\n.byte OTHER\n",
        "dead_undefined": ".if 0\nOTHER=MISSING\n.endif\n.byte 7\n",
        "dead_cycle": ".if 0\nFIRST=SECOND\nSECOND=FIRST\n.endif\n.byte 7\n",
        "backward": "VALUE=3\nOTHER=VALUE+1\n.byte OTHER\n",
    }.items():
        out[name] = source
    return out


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tools = {n: Path("/opt/homebrew/bin") / n for n in ["ca65", "ld65"]}
    files = {}
    records = []
    with tempfile.TemporaryDirectory(prefix="asm198x-attached-label-probe-") as temp:
        root = Path(temp)
        for name, contents in files.items():
            (root / name).write_text(contents)

        def run(argv: list[str]) -> dict:
            try:
                r = subprocess.run(
                    argv, cwd=root, capture_output=True, timeout=10, check=False
                )
            except subprocess.TimeoutExpired as error:
                return {
                    "argv": argv,
                    "cwd": str(root),
                    "exit": 124,
                    "timeout_seconds": 10,
                    "stdout": (error.stdout or b"").decode(errors="replace"),
                    "stderr": (error.stderr or b"").decode(errors="replace"),
                }
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
