"""Capture actual ca65/ld65 file extents and compare a supplied Asm executable."""

import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path


def cases() -> list[dict[str, str]]:
    result = []
    for fill in ("", ",fill=no", ",fill=yes"):
        for name, source, placement in (
            ("one_byte", '.segment "CODE"\n.byte $a5\n', ""),
            ("empty", '.segment "CODE"\n', ""),
            ("absent", ".end\n", ""),
            ("pinned", '.segment "CODE"\n.byte $a5\n', ",start=$1004"),
            ("empty_pinned", '.segment "CODE"\n', ",start=$1004"),
            ("absent_pinned", ".end\n", ",start=$1004"),
            ("alignment", '.segment "CODE"\n.byte $a5\n', ",align=8"),
            ("reserve", '.segment "CODE"\n.res 3\n', ""),
            ("overflow", '.segment "CODE"\n.res 17\n', ""),
        ):
            config = (
                "MEMORY { MAIN:start=$1000,size=$10,file=%O,fillval=$cc"
                + fill
                + "; }\nSEGMENTS { CODE:load=MAIN,type=ro"
                + placement
                + "; }\n"
            )
            result.append(
                {
                    "name": name + (fill or ",default"),
                    "source": source,
                    "config": config,
                }
            )
    for first_fill in ("no", "yes"):
        for second_fill in ("no", "yes"):
            for name, source in (
                (
                    "both",
                    '.segment "CODE"\nfirst_label: .byte $a5\n.segment "TAIL"\nsecond_label: .word first_label,second_label\n',
                ),
                ("empty_first", '.segment "CODE"\n.segment "TAIL"\n.byte $5a\n'),
                ("absent_first", '.segment "TAIL"\n.byte $5a\n'),
                ("trailing_empty", '.segment "CODE"\n.byte $a5\n.segment "TAIL"\n'),
            ):
                config = (
                    "MEMORY { A:start=$1000,size=$10,file=%O,fillval=$cc,fill="
                    + first_fill
                    + "; B:start=$2000,size=$10,file=%O,fillval=$cc,fill="
                    + second_fill
                    + "; }\nSEGMENTS { CODE:load=A,type=ro; TAIL:load=B,type=ro; }\n"
                )
                result.append(
                    {
                        "name": f"areas_{name}_{first_fill}_{second_fill}",
                        "source": source,
                        "config": config,
                    }
                )
    for placement in ("start=$1008", "align=8"):
        result.append(
            {
                "name": "gap_" + placement,
                "source": '.segment "CODE"\n.byte $a5\n.segment "TAIL"\n.byte $5a\n',
                "config": "MEMORY { A:start=$1000,size=$10,file=%O,fillval=$cc; }\nSEGMENTS { CODE:load=A,type=ro; TAIL:load=A,type=ro,"
                + placement
                + "; }\n",
            }
        )
    result.append(
        {
            "name": "ram_does_not_write",
            "source": '.segment "BSS"\n.res 3\n.segment "CODE"\n.word $1234\n',
            "config": 'MEMORY { R:start=$200,size=$10,file=""; A:start=$1000,size=$10,file=%O; }\nSEGMENTS { BSS:load=R,type=bss; CODE:load=A,type=ro; }\n',
        }
    )
    for name, source in (
        ("explicit_zero_reserve", '.segment "CODE"\n.res 3,0\n'),
        ("implicit_align", '.segment "CODE"\n.byte $a5\n.align 8\n.byte $5a\n'),
        ("explicit_zero_align", '.segment "CODE"\n.byte $a5\n.align 8,0\n.byte $5a\n'),
        ("empty_trailing_start", '.segment "CODE"\n.byte $a5\n.segment "TAIL"\n'),
        ("absent_trailing_start", '.segment "CODE"\n.byte $a5\n'),
    ):
        result.append(
            {
                "name": name,
                "source": source,
                "config": "MEMORY { A:start=$1000,size=$10,file=%O,fillval=$cc; }\nSEGMENTS { CODE:load=A,type=ro; TAIL:load=A,type=ro,start=$1008; }\n",
            }
        )
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("assembler", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    assembler = args.assembler.absolute()
    tools = {
        name: Path(shutil.which(name) or name).absolute() for name in ("ca65", "ld65")
    }
    records = []
    with tempfile.TemporaryDirectory(
        prefix="asm198x-next-boundaries-ca65-"
    ) as directory:
        root = Path(directory)

        def run(argv: list[str]) -> dict:
            result = subprocess.run(
                argv, cwd=root, capture_output=True, text=True, timeout=15, check=False
            )
            return {
                "argv": argv,
                "exit": result.returncode,
                "stdout": result.stdout,
                "stderr": result.stderr,
            }

        identities = {}
        for name, tool in tools.items():
            identities[name] = {
                "path": str(tool),
                "sha256": hashlib.sha256(tool.read_bytes()).hexdigest(),
                "version": run([str(tool), "--version"]),
            }
        for case in cases():
            for name in ("native.o", "native.bin", "asm.bin"):
                (root / name).unlink(missing_ok=True)
            (root / "input.s").write_text(case["source"])
            (root / "input.cfg").write_text(case["config"])
            native = [run([str(tools["ca65"]), "input.s", "-o", "native.o"])]
            if native[-1]["exit"] == 0:
                native.append(
                    run(
                        [
                            str(tools["ld65"]),
                            "-C",
                            "input.cfg",
                            "native.o",
                            "-o",
                            "native.bin",
                        ]
                    )
                )
            (root / "asm.bin").write_bytes(b"previous image")
            ours = run(
                [
                    str(assembler),
                    "--dialect",
                    "ca65",
                    "--message-format=json",
                    "-C",
                    "input.cfg",
                    "input.s",
                    "-o",
                    "asm.bin",
                ]
            )
            image = root / "native.bin"
            ours_image = root / "asm.bin"
            record = {
                **case,
                "native": native,
                "native_hex": image.read_bytes().hex() if image.exists() else None,
                "asm": ours,
                "asm_hex": ours_image.read_bytes().hex()
                if ours_image.exists()
                else None,
            }
            if args.check:
                if native[-1]["exit"] == 0:
                    assert ours["exit"] == 0, record
                    assert record["asm_hex"] == record["native_hex"], record
                    formatted = run(
                        [
                            str(assembler),
                            "fmt",
                            "-d",
                            "ca65",
                            "input.s",
                            "-o",
                            "formatted.s",
                        ]
                    )
                    assert formatted["exit"] == 0, formatted
                    repeated = run(
                        [
                            str(assembler),
                            "fmt",
                            "-d",
                            "ca65",
                            "formatted.s",
                            "-o",
                            "twice.s",
                        ]
                    )
                    assert repeated["exit"] == 0, repeated
                    assert (root / "formatted.s").read_bytes() == (
                        root / "twice.s"
                    ).read_bytes(), case
                    rebuilt = run(
                        [
                            str(assembler),
                            "--dialect",
                            "ca65",
                            "-C",
                            "input.cfg",
                            "formatted.s",
                            "-o",
                            "formatted.bin",
                        ]
                    )
                    assert rebuilt["exit"] == 0, rebuilt
                    assert (root / "formatted.bin").read_bytes().hex() == record[
                        "native_hex"
                    ], case
                    record["format"] = {
                        "first": formatted,
                        "second": repeated,
                        "rebuild": rebuilt,
                        "source": (root / "formatted.s").read_text(),
                    }
                else:
                    assert ours["exit"] != 0, record
                    assert ours_image.read_bytes() == b"previous image", record
            records.append(record)
            print(
                case["name"],
                native[-1]["exit"],
                record["native_hex"],
                ours["exit"],
                record["asm_hex"],
                flush=True,
            )
    args.output.write_text(
        json.dumps(
            {
                "tools": identities,
                "assembler_sha256": hashlib.sha256(assembler.read_bytes()).hexdigest(),
                "cases": records,
            },
            indent=2,
        )
        + "\n"
    )


if __name__ == "__main__":
    main()
