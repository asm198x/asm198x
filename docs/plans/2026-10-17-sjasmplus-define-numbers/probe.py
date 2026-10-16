"""Capture SjASMPlus DEFINE identifiers inside numbers and quoted-literal protection."""

import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    expressions = [
        "$FF",
        "#FF",
        "0xFF",
        "0XFF",
        "0b10",
        "0B10",
        "12h",
        "12H",
        "10b",
        "10B",
        "1X",
        "12A",
        "12AA",
        "1X0",
        "1X_0",
        "1X.0",
        "1_X",
        "$_FF",
        "$FOO",
        "%DIGITS",
        "1'0b",
        "$F'F",
        "$F'A'FF",
        "$FF'FF",
        "0F'Fh",
    ]
    definitions = [
        ("FF", "7"),
        ("F", "7"),
        ("xFF", "7"),
        ("XFF", "7"),
        ("b10", "1"),
        ("B10", "1"),
        ("h", "2"),
        ("H", "2"),
        ("b", "1"),
        ("B", "1"),
        ("X", "3"),
        ("A", "3"),
        ("AA", "3"),
        ("_X", "3"),
        ("_FF", "7"),
        ("FOO", "7"),
        ("DIGITS", "101"),
    ]
    # Sample matching and nonmatching complete identifier runs in each spelling.
    for index, expression in enumerate(expressions):
        for name, value in definitions:
            if name in expression or index in [0, 2, 10]:
                out[f"number_{index}_{name}"] = (
                    f" DEFINE {name} {value}\n dd {expression}\n"
                )
    for index, op in enumerate(
        [
            "+",
            "-",
            "*",
            "/",
            "%",
            "<<",
            ">>",
            "&",
            "|",
            "^",
            "==",
            "!=",
            "<",
            "<=",
            ">",
            ">=",
            "&&",
            "||",
        ]
    ):
        out[f"operator_{index}"] = (
            f" DEFINE N 5\nValue equ N{op}2\n dd Value\n IF Value<0\n db $a5\n ELSE\n db $5a\n ENDIF\n ASSERT Value==(N{op}2)\n"
        )
    for count in [1, 2, 18, 19, 20, 21, 22]:
        header = (
            "".join(f" DEFINE V{i} V{i + 1}\n" for i in range(count - 1))
            + f" DEFINE V{count - 1} 7\n"
        )
        out[f"chain_{count}"] = header + " dd $V0\n"
    out.update(
        {
            "original": " DEFINE FF 7\n db $FF,#FF\n",
            "decimal": " DEFINE X 3\n db 1X\n",
            "quoted": ' DEFINE FF 7\n dd "FF",\'F\',"FF"\n',
            "quotes_after_numeric": " DEFINE A 3\n dd $F'A'FF,'A',\"AA\"\n",
            "escaped_quote": ' DEFINE N 3\n db "N\\"N",N\n',
            "escaped_backslash": ' DEFINE N 3\n db "N\\\\",N\n',
            "string_chars": " DEFINE N 3\n db \"N;,//N\",N,';',',','\"'\n",
            "utf8": ' DEFINE N 3\n db "éN",N\n',
            "af": " DEFINE AF 3\n ex af,af'\n db AF\n",
            "af_refusal": " DEFINE af 3\n ex af,af'\n",
            "comment_semicolon": " DEFINE LOOP LOOP\n db 1 ; LOOP\n",
            "comment_slashes": " DEFINE LOOP LOOP\n db 1 // LOOP\n",
            "cycle_self": "Value equ 1\n DEFINE Value Value\n db Value\n",
            "cycle_indirect": " DEFINE X Y\n DEFINE Y X\n db X\n",
            "cycle_growth": " DEFINE X X+X\n dd X\n",
            "chain_prefix": " DEFINE FF N\n DEFINE N 7\n db $FF\n",
            "chain_value": " DEFINE N $FF\n DEFINE FF 7\n db N\n",
            "chained_quotes": ' DEFINE V "FF"\n DEFINE FF 7\n dd V\n',
            "macro": " DEFINE FF 7\n MACRO emit x\n db x,#FF\n ENDM\n emit $FF\n",
            "struct": " DEFINE FF 7\n STRUCT S\nmember BYTE $FF\n ENDS\nthing S {}\n",
            "equ_is_not_define": " DEFINE X Y\nY equ 7\n dd 1X\n",
            "replacement_invalid_base": " DEFINE FF G\n dd $FF\n",
            "replacement_overflow": " DEFINE X 4294967296\n dd 1X\n",
        }
    )
    return out


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tool = Path(shutil.which("sjasmplus") or "/usr/local/bin/sjasmplus").absolute()
    records = []
    with tempfile.TemporaryDirectory(
        prefix="asm198x-define-numbers-native-"
    ) as directory:
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
