"""Retain native ca65/ld65 constant-name token controls."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

CONFIG = "MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { CODE:load=MAIN,type=ro; }\n"


def cases() -> dict[str, str]:
    return {
        "dotted_constant": ".bogus=7\n.byte .bogus\n",
        "ordinary": "VALUE=7\n.byte VALUE\n",
        "underscore": "_VALUE2=7\n.byte _VALUE2\n",
        "underscore_only": "_=7\n.byte _\n",
        "leading_digit": "1VALUE=7\n.byte 1VALUE\n",
        "embedded_dot": "VA.LUE=7\n.byte VA.LUE\n",
        "trailing_dot": "VALUE.=7\n.byte VALUE.\n",
        "cheap_constant": "anchor:\n@value=7\n.byte @value\n",
        "accumulator_upper": "A=7\n.byte A\n",
        "accumulator_lower": "a=7\n.byte a\n",
        "register_x": "X=7\n.byte X\n",
        "register_y": "Y=7\n.byte Y\n",
        "mnemonic": "lda=7\n.byte lda\n",
        "reserved_directive": ".byte=7\n.byte .byte\n",
        "qualified": " .scope outer\n.endscope\nouter::VALUE=7\n.byte outer::VALUE\n",
        "qualified_dot_child": ".scope outer\n.endscope\nouter::.value=7\n.byte outer::.value\n",
        "global_qualified": "::VALUE=7\n.byte ::VALUE\n",
        "global_qualified_dot": "::.value=7\n.byte ::.value\n",
        "scope_plain": ".scope outer\nVALUE=7\n.byte VALUE\n.endscope\n",
        "scope_dot": ".scope outer\n.value=7\n.byte .value\n.endscope\n",
        "dead_branch": ".if 0\n.bogus=7\n.endif\n.byte 7\n",
        "macro_dot": ".macro emit\n.bogus=7\n.byte .bogus\n.endmacro\nemit\n",
        "macro_plain": ".macro emit\nVALUE=7\n.byte VALUE\n.endmacro\nemit\n",
        "repeat_dot": ".repeat 1\n.bogus=7\n.byte .bogus\n.endrepeat\n",
        "include_dot": '.include "invalid.inc"\n',
        "include_plain": '.include "valid.inc"\n',
        "case_sensitive": "Value=7\nVALUE=9\n.byte Value,VALUE\n",
        "digit_suffix": "V123=7\n.byte V123\n",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tools = {n: Path("/opt/homebrew/bin") / n for n in ["ca65", "ld65"]}
    files = {
        "invalid.inc": ".bogus=7\n.byte .bogus\n",
        "valid.inc": "VALUE=7\n.byte VALUE\n",
    }
    records = []
    with tempfile.TemporaryDirectory(prefix="asm198x-constant-names-probe-") as temp:
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
    # Keep distinct observed gaps as evidence, not passing qualification cases.
    deferred_names = {
        "cheap_constant",
        "accumulator_upper",
        "accumulator_lower",
        "register_x",
        "register_y",
        "mnemonic",
        "dead_branch",
    }
    deferred = [r for r in records if r["name"] in deferred_names]
    records = [r for r in records if r["name"] not in deferred_names]
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
