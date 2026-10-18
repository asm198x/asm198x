"""Retain native ca65/ld65 4510 PHD alias controls."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

CONFIG = "MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { CODE:load=MAIN,type=ro; }\n"


def cases() -> dict[str, str]:
    out = {}
    for mnemonic in ["phd", "phw", "PHD", "PhD"]:
        for value in [
            "#$0000",
            "#$00ff",
            "#$0100",
            "#$ffff",
            "$0000",
            "$00ff",
            "$0100",
            "$ffff",
        ]:
            out[f"{mnemonic}_{value}"] = f".p4510\n {mnemonic} {value}\n"
    for mnemonic in ["phd", "phw"]:
        for operand in [
            "#-1",
            "-1",
            "#-32768",
            "-32768",
            "#-32769",
            "-32769",
            "#$10000",
            "$10000",
            "a:$10000",
            "#$1000000",
            "$1000000",
        ]:
            out[f"width_{mnemonic}_{operand}"] = f".p4510\n {mnemonic} {operand}\n"
        for value in ["$ffff", "$10000", "-1", "-32768", "-32769"]:
            for prefix in ["#", ""]:
                out[f"forward_{mnemonic}_{prefix}_{value}"] = (
                    f".p4510\n {mnemonic} {prefix}VALUE\nVALUE={value}\n"
                )
        out[f"org_{mnemonic}_ok"] = f".p4510\n.org $ffff\n {mnemonic} #*\n"
        out[f"org_{mnemonic}_overflow"] = f".p4510\n.org $10000\n {mnemonic} #*\n"
        out[f"label_{mnemonic}_overflow"] = (
            f".p4510\n {mnemonic} target\n.org $10000\ntarget: nop\n"
        )
    out.update(
        {
            "65816_phd": ".p816\n phd\n",
            "65816_reflection": ".p816\n .byte .ismnem(phd),.ismnem(phw)\n",
            "65816_operand_refusal": ".p816\n phd #1\n",
            "reflection": ".p4510\n .byte .ismnem(phd),.ismnemonic(PHD),.ismnem(phw)\n",
            "reflection_base": ".byte .ismnem(phd),.ismnem(phw)\n",
            "reflection_restore": ".p4510\n.pushcpu\n.p02\n .byte .ismnem(phd)\n.popcpu\n .byte .ismnem(phd)\n",
            "base_refusal": " phd #1\n",
            "restore_refusal": ".pushcpu\n.p4510\n.popcpu\n phd #1\n",
            "live_select": ".if 1\n.p4510\n.endif\n phd #1\n",
            "dead_select_refusal": ".if 0\n.p4510\n.endif\n phd #1\n",
            "dead_alias": ".if 0\n phd #1\n.endif\n nop\n",
            "forward": ".p4510\n phd #VALUE\n phd target\ntarget: nop\nVALUE=$1234\n",
            "scope_labels": ".p4510\n.scope inner\nstart: phd finish\nfinish: phd start\n.endscope\n",
            "repeat": ".p4510\n.repeat 3,I\n phd #I\n.endrepeat\n",
            "macro": ".p4510\n.macro push_word value\n phd #value\n.endmacro\n push_word $1234\n",
            "include_debug": '.p4510\nbefore: nop\n.include "push.inc"\nafter: nop\n.word before,after\n',
        }
    )
    for operand in ["", "#$12,x", "$12,x", "($12)", "($12),y"]:
        out[f"syntax_{operand}"] = f".p4510\n phd {operand}\n"
    return out


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tools = {n: Path("/opt/homebrew/bin") / n for n in ["ca65", "ld65"]}
    files = {"push.inc": "inside: phd #$1234\n"}
    records = []
    with tempfile.TemporaryDirectory(prefix="asm198x-4510-phd-probe-") as temp:
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
                "cases": [r for r in records if "_a:" not in r["name"]],
                "address_override_boundary": [r for r in records if "_a:" in r["name"]],
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
