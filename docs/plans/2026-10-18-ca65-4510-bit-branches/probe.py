"""Retain native ca65/ld65 4510 bit-branch controls."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

CONFIG = "MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { CODE:load=MAIN,type=ro; }\n"


def cases() -> dict[str, str]:
    out = {}
    for stem in ["bbr", "bbs"]:
        for bit in range(8):
            for zp in ["$00", "$02", "$ff", "$100", "-1"]:
                out[f"{stem}{bit}_{zp}"] = f".p4510\n {stem}{bit} {zp},*+$34\n"
    for offset in [-129, -128, -127, 0, 1, 2, 3, 127, 128, 129, 130, 131]:
        out[f"pc_{offset}"] = f".p4510\n bbr0 $02,*+({offset})\n"
    for target in ["$0f82", "$0f83", "$1082", "$1083", "*", "(*+2)"]:
        out[f"target_{target}"] = f".p4510\n bbr1 $ff,{target}\n"
    for value in ["0", "2", "3", "127", "128", "130", "131", "$1003", "CONST"]:
        out[f"absolute_{value}"] = f".p4510\nCONST=3\n bbr0 1,{value}\n"
        out[f"org_absolute_{value}"] = f".p4510\n.org $1000\nCONST=3\n bbr0 1,{value}\n"
    for count in [124, 125, 126, 127, 128]:
        out[f"range_forward_{count}"] = (
            f".p4510\n bbr0 1,target\n.res {count}\ntarget: nop\n"
        )
        out[f"range_backward_{count}"] = (
            f".p4510\ntarget: nop\n.res {count}\n bbs0 1,target\n"
        )
    out.update(
        {
            "zp_pc": ".p4510\n.org $20\n bbr0 *,*+3\n",
            "forward": ".p4510\n bbr0 ZP,target\n nop\ntarget: nop\nZP=$ff\n",
            "backward": ".p4510\ntarget: nop\n bbs7 $01,target\n",
            "scope_labels": ".p4510\n.scope inner\nstart: bbr3 7,finish\n bbs4 7,start\nfinish: nop\n.endscope\n",
            "org_reloc_refusal": ".p4510\n.org $1000\n bbr0 1,$1003\n.reloc\n bbr0 1,$1006\n",
            "scope": ".p4510\n.scope inner\nZP=7\nstart: bbr3 ZP,finish\n bbs4 ZP,start\nfinish: nop\n.endscope\n",
            "anonymous": ".p4510\n: bbr0 1,:+\n bbs0 2,:-\n: nop\n",
            "repeat": ".p4510\n.repeat 3,I\n bbr0 I,*+3\n.endrepeat\n",
            "include_debug": '.p4510\nbefore: nop\n.include "branch.inc"\nafter: nop\n.word before,after\n',
            "cpu_restore": ".p4510\n.pushcpu\n.p02\n nop\n.popcpu\n bbs7 7,*+3\n",
            "base_refusal": " bbr0 1,*+3\n",
            "restored_refusal": ".pushcpu\n.p4510\n.popcpu\n bbr0 1,*+3\n",
            "dead": ".if 0\n bbr0 broken,garbage\n.endif\n nop\n",
        }
    )
    for operand in ["", "1", "1,", ",*+3", "1,2,3", "#1,*+3", "(1),*+3", "1,x", "1,y"]:
        out[f"syntax_{operand}"] = f".p4510\n bbr0 {operand}\n"
    return out


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tools = {n: Path("/opt/homebrew/bin") / n for n in ["ca65", "ld65"]}
    files = {"branch.inc": "inside: bbr2 $09,*+3\n"}
    records = []
    with tempfile.TemporaryDirectory(prefix="asm198x-4510-probe-") as temp:
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
