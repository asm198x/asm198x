"""Capture SjASMPlus signed remainder and binary-prefix boundaries."""

import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path


def cases() -> dict[str, str]:
    expressions = {}
    for op in ["%", "MOD", "mod"]:
        for left in [
            "-7",
            "-5",
            "-1",
            "0",
            "1",
            "5",
            "7",
            "$80000000",
            "$ffffffff",
            "$7fffffff",
        ]:
            for right in ["-7", "-2", "-1", "1", "2", "7"]:
                # int32 INT_MIN / -1 (including remainder) is undefined in the
                # native C++ implementation; do not infer portable behavior
                # from this ARM executable's result.
                if left == "$80000000" and right == "-1":
                    continue
                expressions[f"signed_{op}_{left}_{right}"] = f"({left}) {op} ({right})"
    extra = [
        "5%2",
        "5 %2",
        "5% 2",
        "5 % 2",
        "5%10",
        "5%%10",
        "5 % %10",
        "%101%10",
        "%101%%10",
        "%101 % %10",
        "(%101)%(%10)",
        "20%6%3",
        "20/6%2",
        "20%6*3",
        "20*6%7",
        "1+8%3",
        "8%3+1",
        "1<<5%3",
        "8%3<<1",
        "8%3==2",
        "8%3&1",
        "8%3|1",
        "8%3^1",
        "-5%2",
        "5%-2",
        "-5%-2",
        "~5%3",
        "!5%3",
        "(1<<31)%3",
        "($7fffffff+1)%3",
        "($ffffffff+6)%3",
        "($7fffffff*$7fffffff)%3",
        "1'2%5",
        "%1'01%%1'0",
        "0xFFFFFFFF%2",
        "(5%2)%2",
        "5%(2%3)",
    ]
    for index, expression in enumerate(extra):
        expressions[f"expression_{index}"] = expression
    for index, expression in enumerate(
        [
            "5%0",
            "0%0",
            "5 MOD 0",
            "5 mod 0",
            "1 || (5%0)",
            "0 && (5%0)",
            "5%",
            "%2",
            "5%%%10",
            "5 Mod 2",
            "5 mOd 2",
            "5%4294967296",
        ]
    ):
        expressions[f"refusal_{index}"] = expression
    out = {
        name: f"Value equ ({expression})\n dd ({expression}),Value\n IF ({expression})<0\n db $a5\n ELSE\n db $5a\n ENDIF\n ASSERT Value==({expression})\n"
        for name, expression in expressions.items()
    }
    out.update(
        {
            "original": " ASSERT (5 % 2)==1\n db 5 % 2,%101\n",
            "pc": " org 5\nstart: db $%2\n ASSERT ($-start)%2==1\n",
            "symbols": "MOD equ 3\nmod equ 4\nMODX equ 5\n dd MOD,mod,MODX,8 MOD MOD,9 mod mod\n",
            "define": " DEFINE VALUE 5\n DEFINE DIVISOR %10\n db VALUE%DIVISOR\n",
            "prefix_define": " DEFINE DIGITS 101\n db %DIGITS\n",
            "rhs_define": " DEFINE FF 7\n db 5%FF\n",
            "macro": " MACRO emit x,y\n db x%y\n ENDM\n emit 5,%10\n",
            "struct": " STRUCT S\nmember WORD 7%3\nflag BYTE 8 mod 3\n ENDS\nthing S {8%3,7%3}\n ASSERT S==3\n",
            "binary": " db %101,5%10,5%%10,(5)%2,'%'\n",
            "forward": " dd Later%2\nLater equ 5\n",
        }
    )
    return out


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tool = Path(shutil.which("sjasmplus") or "/usr/local/bin/sjasmplus").absolute()
    records = []
    with tempfile.TemporaryDirectory(prefix="asm198x-remainder-native-") as directory:
        root = Path(directory)

        def run(argv: list[str]) -> dict:
            result = subprocess.run(
                argv, cwd=root, capture_output=True, text=True, timeout=10, check=False
            )
            return {
                "argv": argv,
                "exit": result.returncode,
                "stdout": result.stdout,
                "stderr": result.stderr,
            }

        version = run([str(tool), "--version"])
        for name, source in cases().items():
            (root / "input.asm").write_text(source)
            (root / "native.bin").unlink(missing_ok=True)
            record = run([str(tool), "--raw=native.bin", "input.asm"])
            image = root / "native.bin"
            records.append(
                {
                    "name": name,
                    "source": source,
                    **record,
                    "hex": image.read_bytes().hex() if image.exists() else None,
                }
            )
    args.output.write_text(
        json.dumps(
            {
                "tool": str(tool),
                "sha256": hashlib.sha256(tool.read_bytes()).hexdigest(),
                "version": version,
                "cases": records,
            },
            indent=2,
        )
        + "\n"
    )
    print(
        len(records),
        "native cases;",
        sum(record["exit"] == 0 for record in records),
        "accepted",
    )
    for record in records:
        if record["exit"] != 0:
            print(record["name"], record["exit"], record["stderr"].splitlines()[-1])


if __name__ == "__main__":
    main()
