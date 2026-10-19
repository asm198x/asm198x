"""Retain native ca65/ld65 lexical reference phase controls."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

CONFIG = "MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { CODE:load=MAIN,type=ro; }\n"


def cases() -> dict[str, str]:
    out = {}
    for value in ["$12", "$1234"]:
        for later in [None, "$34", "$1234"]:
            tail = "" if later is None else f"VALUE={later}\n"
            for expr in [
                "VALUE",
                "::VALUE",
                "VALUE+250",
                "VALUE*2",
                "<VALUE",
                ">VALUE",
            ]:
                for alias in [False, True]:
                    name = f"value_{value}_later_{later}_{expr}_{alias}"
                    definition = f"OTHER={expr}\n" if alias else ""
                    operand = "OTHER" if alias else expr
                    out[name] = (
                        f"VALUE={value}\n.scope inner\n{definition}lda {operand}\n{tail}.endscope\n"
                    )
    for later in [False, True]:
        for expr in ["VALUE", "::VALUE", "OTHER"]:
            for use in [
                ".byte {x}",
                ".res {x}",
                ".if {x}=2\n.byte 7\n.endif",
                ".byte .defined({x}),.const({x}),.referenced({x})",
            ]:
                name = f"phase_{later}_{expr}_{use}"
                out[name] = (
                    "VALUE=2\n.scope inner\nOTHER=VALUE\n"
                    + use.format(x=expr)
                    + "\n"
                    + ("VALUE=3\n" if later else "")
                    + ".endscope\n"
                )
    for name, source in {
        "alias_updates": ".scope inner\nOTHER=VALUE\nVALUE=2\n.res OTHER\n.endscope\n",
        "alias_chain_updates": ".scope inner\nFIRST=SECOND\nSECOND=VALUE\nVALUE=2\n.res FIRST\n.endscope\n",
        "unused_undefined": "OTHER=MISSING\n.byte 7\n",
        "unused_cycle": "FIRST=SECOND\nSECOND=FIRST\n.byte 7\n",
        "alias_pc": "OTHER=*\n.word OTHER\n",
        "alias_forward_label": "OTHER=target\n.word OTHER\ntarget: .byte 7\n",
        "outer_alias_cycle_shadow": "VALUE=2\n.scope inner\nOTHER=VALUE\nVALUE=OTHER\n.byte OTHER\n.endscope\n",
        "nested_fallback": "VALUE=2\n.scope inner\n.scope nested\n.byte VALUE\n.endscope\nVALUE=3\n.endscope\n",
        "defined_before_ref": "VALUE=2\n.scope inner\n.byte .defined(VALUE)\n.byte VALUE\n.byte .defined(VALUE),.const(VALUE),.referenced(VALUE)\nVALUE=3\n.endscope\n",
        "alias_const_after": "VALUE=2\n.scope inner\nOTHER=VALUE\n.byte .defined(OTHER),.const(OTHER)\nVALUE=3\n.byte .const(OTHER),OTHER\n.endscope\n",
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
