"""Retain native ca65 token-stream/include ordering controls."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

CONFIG = "MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { CODE:load=MAIN,type=ro; }\n"


def cases() -> dict[str, str]:
    return {
        "scope_include_qualified": '.scope inner\n.include "constants.inc"\n.endscope\n.byte inner::VALUE\n',
        "scope_include_global": '.scope inner\n.include "constants.inc"\n.endscope\n.byte ::VALUE\n',
        "repeat_inline_same": ".repeat 2\n.scope\nVALUE=7\n.byte VALUE\n.endscope\n.endrepeat\n",
        "repeat_inline_different": ".repeat 2,I\n.scope\nVALUE=I\n.byte VALUE\n.endscope\n.endrepeat\n",
        "repeat_include_once": '.repeat 1\n.scope\n.include "constants.inc"\n.byte VALUE\n.endscope\n.endrepeat\n',
        "repeat_include_twice": '.repeat 2\n.scope\n.include "constants.inc"\n.byte VALUE\n.endscope\n.endrepeat\n',
        "repeat_include_named": '.repeat 2\n.scope inner\n.include "constants.inc"\n.byte VALUE\n.endscope\n.endrepeat\n',
        "repeat_include_nested": '.repeat 2\n.scope\n.scope\n.include "constants.inc"\n.byte VALUE\n.endscope\n.endscope\n.endrepeat\n',
        "outside_repeat_scope_include": '.scope inner\n.repeat 2\n.include "constants.inc"\n.byte VALUE\n.endrepeat\n.endscope\n',
        "repeat_whole_include": '.repeat 2\n.include "unnamed.inc"\n.endrepeat\n',
        "repeat_include_bare": '.repeat 2\n.include "constants.inc"\n.byte VALUE\n.endrepeat\n',
        "repeat_include_lateglobal": '.repeat 1\n.scope\n.include "constants.inc"\n.endscope\n.endrepeat\n.byte ::VALUE\n',
        "repeat_include_existing_global": 'VALUE=9\n.repeat 1\n.scope\n.include "constants.inc"\n.byte VALUE\n.endscope\n.endrepeat\n',
        "repeat_include_external": '.repeat 2\n.scope\n.include "code.inc"\n.endscope\n.endrepeat\n',
        "repeat_order": '.repeat 2,I\n.byte I\n.include "code.inc"\n.endrepeat\n.byte 9\n',
        "repeat_lifo": '.repeat 1\n.include "one.inc"\n.include "two.inc"\n.byte 3\n.endrepeat\n.byte 9\n',
        "repeat_scope_late": '.repeat 1\n.scope\n.include "constants.inc"\n.endscope\n.endrepeat\n.byte ::VALUE\n',
        "repeat_nested_scope": '.repeat 1\n.scope outer\n.repeat 1\n.include "constants.inc"\n.endrepeat\n.endscope\n.endrepeat\n.byte ::VALUE\n',
        "macro_order": '.macro emit\n.byte 1\n.include "code.inc"\n.byte 2\n.endmacro\nemit\n.byte 9\n',
        "macro_scope": '.macro emit\n.scope\n.include "constants.inc"\n.endscope\n.endmacro\nemit\n.byte ::VALUE\n',
        "macro_counter": '.macro emit I\n.include "counter.inc"\n.endmacro\nemit 7\n',
        "conditional_order": '.if 1\n.byte 1\n.include "code.inc"\n.byte 2\n.endif\n.byte 9\n',
        "repeat_zero_include": '.repeat 0\n.include "missing.inc"\n.endrepeat\n.byte 9\n',
        "repeat_nested_order": '.repeat 2,I\n.byte I\n.repeat 1,J\n.include "code.inc"\n.byte 3\n.endrepeat\n.byte 4\n.endrepeat\n.byte 9\n',
        "pending_file_repeated": '.repeat 3\n.include "one.inc"\n.byte 3\n.endrepeat\n',
        "pending_search_path": '.repeat 1\n.include "dir/first.inc"\n.include "pick.inc"\n.byte 3\n.endrepeat\n',
        "pending_incbin_path": '.repeat 1\n.include "dir/first.inc"\n.incbin "data.dat"\n.byte 3\n.endrepeat\n',
        "pending_child_reuses_sibling": '.repeat 1\n.include "one.inc"\n.include "child.inc"\n.byte 3\n.endrepeat\n',
        "repeat_condition_before_include": '.repeat 1\n.include "constants.inc"\n.if VALUE\n.byte 3\n.endif\n.endrepeat\n',
        "macro_condition_before_include": '.macro emit\n.include "constants.inc"\n.if VALUE\n.byte 3\n.endif\n.endmacro\nemit\n',
        "repeat_live_text": '.define V 1\n.repeat 1\n.include "text.inc"\n.undefine V\n.define V 2\n.endrepeat\n',
        "macro_live_text": '.define V 1\n.macro emit\n.include "text.inc"\n.undefine V\n.define V 2\n.endmacro\nemit\n',
        "incbin_immediate": '.repeat 2,I\n.byte I\n.incbin "data.dat"\n.byte 3\n.endrepeat\n',
        "macro_each_call": '.macro emit\n.include "one.inc"\n.byte 2\n.endmacro\nemit\nemit\n',
        "macro_nested": '.macro inner\n.include "one.inc"\n.endmacro\n.macro outer\ninner\n.byte 2\n.endmacro\nouter\n.byte 9\n',
        "macro_in_repeat": '.macro emit value\n.include "one.inc"\n.byte value\n.endmacro\n.repeat 2,I\nemit I\n.endrepeat\n.byte 9\n',
        "repeat_in_macro": '.macro emit\n.repeat 2,I\n.include "one.inc"\n.byte I\n.endrepeat\n.byte 3\n.endmacro\nemit\n.byte 9\n',
        "pending_file_with_macro": '.repeat 1\n.include "one.inc"\n.include "macro.inc"\n.byte 3\n.endrepeat\n.byte 9\n',
        "dead_macro_include": '.macro emit\n.if 0\n.include "missing.inc"\n.endif\n.byte 3\n.endmacro\nemit\n',
        "dead_repeat_include": '.repeat 2,I\n.if 0\n.include "missing.inc"\n.endif\n.byte I\n.endrepeat\n',
        "missing_queued_file": '.repeat 1\n.include "missing.inc"\n.byte 3\n.endrepeat\n',
        "queued_include_label": '.repeat 1\nplace: .include "one.inc"\n.byte 3\n.endrepeat\n.word place\n',
        "repeat_live_constant": '.repeat 1\n.include "text.inc"\nV=2\n.endrepeat\n',
        "macro_live_constant": '.macro emit\n.include "text.inc"\nV=2\n.endmacro\nemit\n',
        "pending_incbin_fallback": '.repeat 1\n.include "dir/first.inc"\n.incbin "only.dat"\n.byte 3\n.endrepeat\n',
        "include_debug": 'before: nop\n.macro emit\n.include "one.inc"\n.byte 3\n.endmacro\nemit\nafter: nop\n.word before,after\n',
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tools = {n: Path("/opt/homebrew/bin") / n for n in ["ca65", "ld65"]}
    files = {
        "constants.inc": "VALUE=7\n",
        "unnamed.inc": ".scope\nVALUE=7\n.byte VALUE\n.endscope\n",
        "code.inc": ".byte 7\n",
        "one.inc": ".byte 1\n",
        "two.inc": ".byte 2\n",
        "counter.inc": ".byte I\n",
        "dir/first.inc": ".byte $a1\n",
        "dir/pick.inc": ".byte $a3\n",
        "pick.inc": ".byte $a2\n",
        "dir/data.dat": "D",
        "dir/only.dat": "D",
        "data.dat": "R",
        "text.inc": ".byte V\n",
        "child.inc": '.include "one.inc"\n.byte 2\n',
        "macro.inc": '.macro emit\n.include "one.inc"\n.byte 2\n.endmacro\nemit\n.byte 4\n',
        "wrapped.inc": ".scope inner\nVALUE=7\n nop\n.endscope\n",
    }
    records = []
    with tempfile.TemporaryDirectory(prefix="asm198x-scoped-constants-probe-") as temp:
        root = Path(temp)
        for name, contents in files.items():
            (root / name).parent.mkdir(parents=True, exist_ok=True)
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
    deferred = [
        r for r in records if r["name"] in {"repeat_live_text", "macro_live_text"}
    ]
    records = [r for r in records if r not in deferred]
    (Path(__file__).parent / "deferred-native.json").write_text(
        json.dumps({"tools": identities, "files": files, "cases": deferred}, indent=2)
        + "\n"
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
