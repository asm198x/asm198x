"""Retain native ca65/ld65 4510 selection and indirect-Z controls."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

CONFIG = "MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { CODE:load=MAIN,type=ro; }\n"


def cases() -> dict[str, str]:
    out = {}
    for cpu in ["4510", "6502", "65c02"]:
        for mnemonic in ["ora", "and", "eor", "adc", "sta", "lda", "cmp", "sbc"]:
            for value in ["$07", "$ff", "$100", "-1"]:
                out[f"{cpu}_{mnemonic}_{value}"] = (
                    f'.setcpu "{cpu}"\n {mnemonic} ({value}),z\n'
                )
    out.update(
        {
            "default_refusal": " ora ($07),z\n",
            "p4510": ".p4510\n ora ($07),z\n",
            "selector_case": '.setcpu "4510"\n ORA ($07),Z\n',
            "base_modes": '.setcpu "4510"\n ora #1\n ora $07\n ora $1234\n ora ($07,x)\n ora ($07),y\n',
            "restore": '.pushcpu\n.setcpu "4510"\n ora ($07),z\n.popcpu\n ora #2\n',
            "restore_refusal": '.pushcpu\n.setcpu "4510"\n ora ($07),z\n.popcpu\n ora ($07),z\n',
            "nested_restore": '.setcpu "4510"\n.pushcpu\n.setcpu "6502"\n.pushcpu\n.setcpu "4510"\n ora ($07),z\n.popcpu\n ora #2\n.popcpu\n ora ($09),z\n',
            "empty_pop": ".popcpu\n ora #1\n",
            "dead_switch": '.if 0\n.setcpu "4510"\n.endif\n ora ($07),z\n',
            "live_switch": '.if 1\n.setcpu "4510"\n.endif\n ora ($07),z\n',
            "dead_invalid": '.if 0\n.setcpu "nonsense"\n.endif\n ora #1\n',
            "selector_unknown": '.setcpu "nonsense"\n ora #1\n',
            "selector_4502": '.setcpu "4502"\n ora #1\n',
            "selector_unquoted": ".setcpu 4510\n ora #1\n",
            "conditional_cpu": '.setcpu "4510"\n.ifp4510\n .byte 1\n.endif\n.ifp02\n .byte 2\n.endif\n',
            "conditional_restored": '.setcpu "4510"\n.pushcpu\n.p02\n.ifp02\n .byte 2\n.endif\n.popcpu\n.ifp4510\n .byte 1\n.endif\n',
            "new_mnemonics": '.setcpu "4510"\n cle\n see\n ldz #$07\n map\n eom\n',
            "reflection": '.setcpu "4510"\n .byte .ismnem(ldz),.ismnem(ora),.ismnem(aug),.ismnem(map)\n',
            "zp_symbol": '.setcpu "4510"\nVALUE=$07\n ora (VALUE),z\n',
            "forward_zp": '.setcpu "4510"\n ora (VALUE),z\nVALUE=$07\n',
        }
    )
    out.update(
        {
            "nested_reflection": ".p4510\n .byte .ismnem(ldz)+.ismnem(map)\n .byte .match({z},{q}),.xmatch({z},{q})\n",
            "reflection_restore": ".p4510\n .byte .ismnem(ldz)\n.p02\n .byte .ismnem(ldz)\n",
            "reflection_constant": '.p4510\nN = .ismnem(ldz)\n .byte N\n .byte .sprintf("%d",N)\n',
            "dead_reflection": ".if 0\n.p4510\n.endif\n .byte .ismnem(ldz)\n",
            "repeat_selection": ".repeat 2\n.p4510\n ora ($07),z\n.p02\n.endrepeat\n ora #2\n",
            "include_select": '.include "select.inc"\n ora ($09),z\n',
            "include_restore": '.pushcpu\n.include "select.inc"\n.popcpu\n ora #2\n',
            "include_restore_refusal": '.pushcpu\n.include "select.inc"\n.popcpu\n ora ($09),z\n',
            "long_forward": ".p4510\n lbne target\n nop\ntarget: nop\n",
            "long_backward": ".p4510\ntarget: nop\n lbne target\n",
            "word_immediate": ".p4510\n phw #$1234\n",
            "stack_indirect": ".p4510\n lda ($07,s),y\n sta ($09,s),y\n",
            "absolute_indirect": ".p4510\n jsr ($1234)\n jsr ($2345,x)\n",
            "negative_pop_args": ".pushcpu 1\n ora #1\n",
        }
    )
    for cpu in ["6502", "4510"]:
        for register in ["z", "s", "b"]:
            out[f"token_{cpu}_{register}"] = (
                f'.setcpu "{cpu}"\n .byte .match({{{register}}},{{q}})\n'
            )
    out["repeat_selected"] = ".p4510\n.repeat 2\n ora ($07),z\n.endrepeat\n"
    out["repeat_restore_base"] = (
        ".p4510\n.repeat 2\n.p02\n ora #2\n.p4510\n ora ($07),z\n.endrepeat\n"
    )
    out["include_debug"] = (
        'before: ora #1\n.include "select.inc"\nafter: ora ($09),z\n.word before,after\n'
    )
    return out


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tools = {n: Path("/opt/homebrew/bin") / n for n in ["ca65", "ld65"]}
    files = {"select.inc": '.setcpu "4510"\n ora ($07),z\n'}
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
