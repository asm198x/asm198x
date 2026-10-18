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
    for cpu in ["6502", "4510"]:
        for prefix in ["z:", "a:", "f:", "A : ", "q:", "A:", "a: ", "Z:"]:
            for operand in ["$12", "$1234", "$10000", "-1"]:
                out[f"{cpu}_{prefix}_{operand}"] = (
                    f'.setcpu "{cpu}"\n lda {prefix}{operand}\n'
                )
    for operand in [
        "a:$12,x",
        "a:$12,y",
        "z:$12,x",
        "z:$12,y",
        "a:($12,x)",
        "z:($12,x)",
        "a:($12),y",
        "z:($12),y",
        "a:#$12",
        "z:#$12",
        "#a:$12",
        "#z:$12",
        "a:",
        "a::",
        "a: :",
        ":",
        "::",
        ":::",
    ]:
        out[f"syntax_{operand}"] = f" lda {operand}\n"
    for mnemonic in ["jsr", "jmp", "phw", "phd", "bbr0"]:
        for prefix in ["z:", "a:", "f:"]:
            tail = ",*+3" if mnemonic == "bbr0" else ""
            out[f"4510_{mnemonic}_{prefix}"] = (
                f".p4510\n {mnemonic} {prefix}$12{tail}\n"
            )
    out.update(
        {
            "old_phd_override_overflow": ".p4510\n phd a:$10000\n",
            "old_phw_override_overflow": ".p4510\n phw a:$10000\n",
            "65816_a": ".p816\n lda a:$12\n",
            "65816_z": ".p816\n lda z:$12\n",
            "65816_f": ".p816\n lda f:$12\n",
            "forward_a": " lda a:VALUE\nVALUE=$12\n",
            "forward_z": " lda z:VALUE\nVALUE=$12\n",
            "forward_z_overflow": " lda z:VALUE\nVALUE=$1234\n",
            "scope": ".scope inner\nlabel: nop\n.endscope\n lda a:inner::label\n",
            "global": "label: nop\n.scope inner\n lda a:::label\n.endscope\n",
            "scoped_plain": ".scope inner\nlabel: nop\n.endscope\n lda inner::label\n",
            "colon_data": " .byte foo: \n",
            "colon_only": " .byte : \n",
            "include_debug": 'before: nop\n.include "size.inc"\nafter: nop\n.word before,after\n',
            "repeat": ".repeat 3,I\n lda a:I\n.endrepeat\n",
            "dead": ".if 0\n lda a:broken\n.endif\n nop\n",
        }
    )
    return out


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tools = {n: Path("/opt/homebrew/bin") / n for n in ["ca65", "ld65"]}
    files = {"size.inc": "inside: lda a:$12\n"}
    records = []
    with tempfile.TemporaryDirectory(prefix="asm198x-address-size-probe-") as temp:
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
