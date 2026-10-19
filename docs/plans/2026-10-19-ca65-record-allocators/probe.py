"""Retain native ca65/ld65 record field allocation controls."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

CONFIG = "MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { CODE:load=MAIN,type=ro; }\n"


def cases() -> dict[str, str]:
    out = {}
    for kind in ["struct", "union"]:
        for allocator in [
            "byte",
            "word",
            "addr",
            "dbyt",
            "faraddr",
            "dword",
            "res",
            "tag_empty",
            "tag_item",
        ]:
            for named in [False, True]:
                for count in [None, -1, 0, 1, 2, 65535, 65536, 16777215, 16777216]:
                    prefix = ""
                    word = allocator
                    args = ""
                    if allocator.startswith("tag_"):
                        item = "Empty" if allocator == "tag_empty" else "Item"
                        prefix = ".struct Empty\n.endstruct\n.struct Item\nfield .word\n.endstruct\n"
                        word = "tag"
                        args = " " + item
                    if count is not None:
                        args += " " + str(count)
                    field = "field " if named else ""
                    source = (
                        prefix
                        + f".{kind} Record\n{field}.{word}{args}\n.end{kind}\n.dword .sizeof(Record)\n"
                    )
                    if named:
                        source += ".dword .sizeof(Record::field)\n"
                    name = f"{kind}_{allocator}_{'named' if named else 'anonymous'}_{'default' if count is None else count}"
                    out[name] = source
    for kind in ["struct", "union"]:
        for allocator, unit in [
            ("word", 2),
            ("addr", 2),
            ("dbyt", 2),
            ("faraddr", 3),
            ("dword", 4),
            ("tag_item", 2),
            ("tag_empty", 0),
        ]:
            limit = 0xFFFFFF // unit if unit else 0x7FFFFFFF
            for named in [False, True]:
                for count in [limit, limit + 1]:
                    prefix = ""
                    word = allocator
                    args = ""
                    if allocator.startswith("tag_"):
                        item = "Empty" if not unit else "Item"
                        prefix = ".struct Empty\n.endstruct\n.struct Item\nfield .word\n.endstruct\n"
                        word = "tag"
                        args = " " + item
                    field = "field " if named else ""
                    source = (
                        prefix
                        + f".{kind} Record\n{field}.{word}{args} {count}\n.end{kind}\n.dword .sizeof(Record)\n"
                    )
                    if named:
                        source += ".dword .sizeof(Record::field)\n"
                    out[
                        f"{kind}_{allocator}_{'named' if named else 'anonymous'}_boundary_{count}"
                    ] = source
    for name, body in {
        "dead_zero": ".struct Record\nfield .byte 0\n.endstruct\n",
        "dead_negative": ".union Record\nfield .word -1\n.endunion\n",
        "dead_missing_res": ".struct Record\nfield .res\n.endstruct\n",
        "dead_oversize": ".struct Record\nfield .dword 16777215\n.endstruct\n",
    }.items():
        out[name] = ".if 0\n" + body + ".endif\n.byte 7\n"
    for kind in ["struct", "union"]:
        out[f"{kind}_empty_tag_beyond_32_bits"] = (
            f".struct Empty\n.endstruct\n.{kind} Record\nfield .tag Empty 4294967296\n.end{kind}\n.dword .sizeof(Record), .sizeof(Record::field)\n"
        )
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
