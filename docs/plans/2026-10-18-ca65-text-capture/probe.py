"""Retain native ca65 definition and invocation token capture controls."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

CONFIG = "MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { CODE:load=MAIN,type=ro; }\n"


def cases() -> dict[str, str]:
    return {
        "plain_undef": ".define VALUE 1\n.undefine VALUE\n.define VALUE 2\n.byte VALUE\n",
        "conditional_undef": ".define VALUE 1\n.if 1\n.undefine VALUE\n.endif\n.define VALUE 2\n.byte VALUE\n",
        "repeat_prior_undef": ".define VALUE 1\n.repeat 1\n.undefine VALUE\n.endrepeat\nnop\n",
        "macro_prior_undef": ".define VALUE 1\n.macro emit\n.undefine VALUE\n.endmacro\nemit\nnop\n",
        "macro_snapshot": ".define VALUE 1\n.macro emit\n.byte VALUE\n.endmacro\n.undefine VALUE\n.define VALUE 2\nemit\n",
        "macro_late_define": ".macro emit\n.byte VALUE\n.endmacro\n.define VALUE 1\nemit\n",
        "macro_late_two_calls": ".macro emit\n.byte VALUE\n.endmacro\n.define VALUE 1\nemit\n.undefine VALUE\n.define VALUE 2\nemit\n",
        "macro_prior_two_calls": ".define VALUE 1\n.macro emit\n.byte VALUE\n.endmacro\nemit\n.undefine VALUE\n.define VALUE 2\nemit\n",
        "macro_never_called": ".define VALUE 1\n.macro emit\n.undefine VALUE\n.endmacro\nnop\n",
        "macro_dead_call": ".define VALUE 1\n.macro emit\n.undefine VALUE\n.endmacro\n.if 0\nemit\n.endif\nnop\n",
        "repeat_define_inside": ".repeat 2\n.define VALUE 1\n.byte VALUE\n.undefine VALUE\n.endrepeat\n",
        "repeat_prior_zero": ".define VALUE 1\n.repeat 0\n.undefine VALUE\n.endrepeat\n.byte VALUE\n",
        "repeat_prior_dead_branch": ".define VALUE 1\n.repeat 1\n.if 0\n.undefine VALUE\n.endif\n.byte VALUE\n.endrepeat\n",
        "conditional_before_macro": ".if 1\n.define VALUE 1\n.endif\n.macro emit\n.byte VALUE\n.endmacro\n.undefine VALUE\n.define VALUE 2\nemit\n",
        "macro_argument_capture": ".define VALUE 1\n.macro emit arg\n.byte VALUE,arg\n.endmacro\n.undefine VALUE\n.define VALUE 2\nemit VALUE\n",
        "macro_nested_capture": ".define VALUE 1\n.macro inner\n.byte VALUE\n.endmacro\n.macro outer\ninner\n.endmacro\n.undefine VALUE\n.define VALUE 2\nouter\n",
        "macro_param_capture_refusal": ".define VALUE 1\n.macro emit VALUE\n.byte VALUE\n.endmacro\nemit 7\n",
        "macro_duplicate_refusal": ".macro emit\n.byte 1\n.endmacro\n.macro emit\n.byte 2\n.endmacro\nemit\n",
        "macro_nested_definition": ".macro outer\n.macro inner\n.byte 7\n.endmacro\n.endmacro\nouter\ninner\n",
        "macro_forward_call_refusal": "emit\n.macro emit\n.byte 7\n.endmacro\n",
        "macro_body_later_definition": ".macro outer\ninner\n.endmacro\n.macro inner\n.byte 7\n.endmacro\nouter\n",
        "macro_selected_definition": ".if 1\n.macro emit\n.byte 7\n.endmacro\n.endif\nemit\n",
        "macro_dead_definition_refusal": ".if 0\n.macro emit\n.byte 7\n.endmacro\n.endif\nemit\n",
        "macro_unique_locals_repeat": ".macro emit\n.local loop\nloop: nop\nbne loop\n.endmacro\n.repeat 2\nemit\n.endrepeat\n",
        "include_definition_snapshot": '.define VALUE 1\n.include "macros.inc"\n.undefine VALUE\n.define VALUE 2\nincluded\n',
        "include_invocation": '.macro emit\n.byte 7\n.endmacro\n.include "call.inc"\n',
        "include_late_definition_refusal": '.include "call.inc"\n.macro emit\n.byte 7\n.endmacro\n',
        "include_siblings": '.include "emit.inc"\n.include "call.inc"\n',
        "include_nested_definition": '.define VALUE 7\n.include "nested.inc"\nincluded\n',
        "definedmacro_order": ".byte .definedmacro(emit)\n.macro emit\n.byte 7\n.endmacro\n.byte .definedmacro(emit)\nemit\n",
        "definedmacro_body_later": ".macro outer\n.byte .definedmacro(inner)\n.endmacro\n.macro inner\n.byte 7\n.endmacro\nouter\n",
        "macro_quote_snapshot": '.define VALUE "X"\n.macro emit arg\n.byte VALUE,arg,"VALUE"\n.endmacro\n.undefine VALUE\n.define VALUE "Y"\nemit VALUE\n',
        "macro_too_many_refusal": ".macro emit arg\n.byte arg\n.endmacro\nemit 1,2\n",
        "macro_unused_missing_argument": ".macro emit arg\n.byte 7\n.endmacro\nemit\n",
        "macro_recursive_refusal": ".macro emit\nemit\n.endmacro\nemit\n",
        "macro_capture_counter": ".repeat 1,I\n.macro emit\n.byte I\n.endmacro\n.endrepeat\nemit\n",
        "dead_unclosed_macro": ".if 0\n.macro emit\n.endif\nnop\n",
        "macro_unclosed_refusal": ".macro emit\n.byte 7\n",
        "macro_name_text": ".define NAME emit\n.macro NAME\n.byte 7\n.endmacro\nemit\n",
        "macro_body_dead_syntax": ".macro emit\n.if 0\nundefined!syntax\n.endif\n.byte 7\n.endmacro\nemit\n",
        "macro_labeled_call": ".macro emit value\n.byte value\n.endmacro\nstart: emit 7\n.word start\n",
        "macro_own_input_context": ".macro emit\n.undefine VALUE\n.define VALUE 2\n.byte VALUE\n.endmacro\n.define VALUE 1\n.repeat 1\nemit\n.endrepeat\n",
        "macro_bare_counter_refusal": ".macro emit\n.byte I\n.endmacro\n.repeat 2,I\nemit\n.endrepeat\n",
        "macro_counter_global_fallback": "I=9\n.macro emit\n.byte I\n.endmacro\n.repeat 2,I\nemit\n.endrepeat\n",
        "macro_counter_condition_refusal": ".macro emit\n.if I\n.byte 7\n.endif\n.endmacro\n.repeat 2,I\nemit\n.endrepeat\n",
        "macro_counter_count_refusal": ".macro emit\n.repeat I\n.byte 7\n.endrepeat\n.endmacro\n.repeat 2,I\nemit\n.endrepeat\n",
        "macro_counter_constant_refusal": ".macro emit\n.scope\nVALUE=I\n.byte VALUE\n.endscope\n.endmacro\n.repeat 2,I\nemit\n.endrepeat\n",
        "repeat_generated_counter": '.repeat 2,I\n.byte .ident("I")\n.endrepeat\n',
        "macro_counter_parameter": ".macro emit value\n.byte value\n.endmacro\n.repeat 2,I\nemit I\n.endrepeat\n",
        "include_debug": 'before: nop\n.define VALUE 1\n.include "macros.inc"\n.undefine VALUE\n.define VALUE 2\nincluded\nafter: nop\n.word before,after\n',
        "repeat_redefine_in_deadbranch": ".define VALUE 1\n.repeat 1\n.if 0\n.undefine VALUE\n.define VALUE 2\n.endif\n.byte VALUE\n.endrepeat\n",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tools = {n: Path("/opt/homebrew/bin") / n for n in ["ca65", "ld65"]}
    files = {
        "constants.inc": "VALUE=7\n",
        "macros.inc": ".macro included\n.byte VALUE\n.endmacro\n",
        "emit.inc": ".macro emit\n.byte 7\n.endmacro\n",
        "call.inc": "emit\n",
        "nested.inc": '.include "macros.inc"\n',
        "wrapped.inc": ".scope inner\nVALUE=7\n nop\n.endscope\n",
    }
    records = []
    with tempfile.TemporaryDirectory(prefix="asm198x-scoped-constants-probe-") as temp:
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
    records.sort(key=lambda record: record["name"] != "macro_argument_capture")
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
