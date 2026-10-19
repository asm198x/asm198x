"""Retain native ca65/ld65 macro introspection token controls."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

CONFIG = "MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { CODE:load=MAIN,type=ro; }\n"


def cases() -> dict[str, str]:
    return {
        "defined_literal": '.macro emit\n.byte 7\n.endmacro\n.byte ".definedmacro(emit)"\n',
        "defined_literal_unknown": '.byte ".definedmacro(missing)"\n',
        "defined_uppercase": ".macro emit\n.byte 7\n.endmacro\n.byte .DEFINEDMACRO(emit)\n",
        "defined_mixedcase": ".macro emit\n.byte 7\n.endmacro\n.byte .DefinedMacro(emit)\n",
        "paramcount_literal": '.macro emit one,two\n.byte ".paramcount",.paramcount\n.endmacro\nemit 1,2\n',
        "paramcount_uppercase": ".macro emit value\n.byte .PARAMCOUNT\n.endmacro\nemit 7\n",
        "paramcount_mixedcase": ".macro emit value\n.byte .ParamCount\n.endmacro\nemit 7\n",
        "defined_late_text_name": ".macro emit\n.byte .definedmacro(NAME)\n.endmacro\n.macro target\n.byte 7\n.endmacro\n.define NAME target\nemit\n",
        "defined_argument_text_name": ".macro emit value\n.byte value\n.endmacro\n.macro target\n.byte 7\n.endmacro\n.define NAME target\nemit .definedmacro(NAME)\n",
        "defined_name_case": ".macro emit value\n.byte 7\n.endmacro\n.byte .definedmacro(emit),.definedmacro(EMIT)\n",
        "defined_prefix_symbol": ".byte .definedmacroSuffix(emit)\n",
        "paramcount_suffix_symbol": ".macro emit\n.byte .paramcountSuffix\n.endmacro\nemit\n",
        "defined_comment": ".macro emit value\n.byte 7\n.endmacro\n.byte 7 ; .definedmacro(no_close\n",
        "paramcount_comment": ".macro emit\n.byte 7 ; .paramcount\n.endmacro\nemit\n",
        "defined_two_queries": ".macro emit value\n.byte 7\n.endmacro\n.byte .definedmacro(emit),.definedmacro(missing)\n",
        "defined_single_quotes": ".macro emit value\n.byte 7\n.endmacro\n.byte '.'\n",
        "defined_utf8_literal": '.macro emit value\n.byte 7\n.endmacro\n.byte "é.definedmacro(emit)"\n',
        "paramcount_utf8_literal": '.macro emit value\n.byte "é.paramcount",.paramcount\n.endmacro\nemit 7\n',
        "defined_late_alias_false": ".macro emit\n.byte .definedmacro(NAME)\n.endmacro\n.define NAME missing\nemit\n",
        "defined_late_alias_rebind": ".macro emit\n.byte .definedmacro(NAME)\n.endmacro\n.macro target\n.byte 7\n.endmacro\n.define NAME target\nemit\n.undefine NAME\n.define NAME missing\nemit\n",
        "defined_parameter": ".macro emit value\n.byte .definedmacro(value)\n.endmacro\n.macro target\n.byte 7\n.endmacro\nemit target\n",
        "defined_dead_branch": ".if 0\n.byte .definedmacro(\n.endif\n.byte 7\n",
        "defined_missing_close": ".macro emit value\n.byte 7\n.endmacro\n.byte .definedmacro(emit\n",
        "defined_missing_name": ".macro emit value\n.byte 7\n.endmacro\n.byte .definedmacro()\n",
        "paramcount_outside": ".byte .paramcount\n",
        "defined_nested_alias": ".macro inner\n.byte .definedmacro(NAME)\n.endmacro\n.macro outer\ninner\n.endmacro\n.macro target\n.byte 7\n.endmacro\n.define NAME target\nouter\n",
        "defined_body_condition": ".macro emit\n.if .definedmacro(NAME)\n.byte 7\n.else\n.byte 9\n.endif\n.endmacro\n.macro target\n.byte 7\n.endmacro\n.define NAME target\nemit\n",
        "defined_body_repeat": ".macro emit\n.repeat .definedmacro(NAME)\n.byte 7\n.endrepeat\n.endmacro\n.macro target\n.byte 7\n.endmacro\n.define NAME target\nemit\n",
        "defined_body_live_definition": ".macro emit\n.define NAME target\n.byte .definedmacro(NAME)\n.undefine NAME\n.endmacro\n.macro target\n.byte 7\n.endmacro\nemit\n",
        "paramcount_nested_calls": ".macro inner value\n.byte .PaRaMcOuNt\n.endmacro\n.macro outer one,two\n.byte .PARAMCOUNT\ninner 7\n.endmacro\nouter 1,2\n",
        "defined_formal_case": ".macro emit value\n.byte .definedmacro(value),.definedmacro(VALUE)\n.endmacro\n.macro target\n.byte 7\n.endmacro\nemit target\n",
        "defined_includes": '.include "definitions.inc"\n.byte .DEFINEDMACRO(target),.definedmacro(TARGET)\n',
        "paramcount_missing_arguments": ".macro emit one,two\n.byte .PARAMCOUNT\n.endmacro\nemit\n",
        "paramcount_keyword_boundary": '.macro emit value\n.byte ".PARAMCOUNT_SUFFIX",.PARAMCOUNT\n.endmacro\nemit 7\n',
        "defined_whitespace": ".macro target\n.byte 7\n.endmacro\n.byte .DEFINEDMACRO ( target )\n",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tools = {n: Path("/opt/homebrew/bin") / n for n in ["ca65", "ld65"]}
    files = {"definitions.inc": ".macro target\n.byte 7\n.endmacro\n"}
    records = []
    with tempfile.TemporaryDirectory(
        prefix="asm198x-macro-introspection-probe-"
    ) as temp:
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
