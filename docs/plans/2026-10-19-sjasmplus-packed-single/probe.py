"""Retain native packed single-quote data and numeric expression boundaries."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    literals = [
        "''",
        "'F'",
        "'FF'",
        "'ABC'",
        "'ABCD'",
        "'ABCDE'",
        "'ABCDEFGH'",
        "''''",
        "'a''b'",
        "'\\'",
        "'\\n'",
        "'é'",
        "'éé'",
        "'😀'",
        "';'",
        "'//'",
        "','",
        "'a:b'",
        "'\"'",
    ]
    for index, literal in enumerate(literals):
        for directive in ["db", "dw", "d24", "dd", "dc"]:
            out[f"data_{directive}_{index}"] = f" {directive} {literal}\n"
        out[f"scalar_{index}"] = f"value equ {literal}\n dd value\n"
        out[f"arithmetic_{index}"] = f"value equ {literal}+1\n dd value\n"
        out[f"parenthesized_{index}"] = f" dd ({literal})\n"
    for index, literal in enumerate(["'F'", "'AB'", "''''", "'éé'"]):
        for directive in ["db", "dw", "dd"]:
            for expression in [literal + "+1", "(" + literal + "+1)"]:
                out[f"tail_{directive}_{index}_{expression}"] = (
                    f" {directive} {expression}\n"
                )
    for count in range(2, 11):
        out[f"apostrophes_{count}"] = " dd " + ("'" * count) + "\n"
    out.update(
        {
            "if_value": " IF 'AB'=$4142\n db 7\n ELSE\n db 8\n ENDIF\n",
            "shift_sign": " dd 'éé'>>24\n dd ('éé'&$80000000)!=0\n",
            "instruction": " ld hl,'AB'\n ld a,'F'\n",
            "define_literal": " DEFINE FF 7\n dd 'FF'\n",
            "define_numeric": " DEFINE FF 7\n dd $FF,1'000\n",
            "af_register": " ex af,af'\n dd 'AB'\n",
            "macro_argument": "put MACRO value\n dd value\n ENDM\n put 'AB'\n",
            "repeat": " DUP 2\n dd 'AB'\n EDUP\n",
            "delimiter_comment": " dd ';,://','AB' ; real comment with 'bad\n",
            "malformed_unclosed": " dd 'ABC\n",
            "malformed_single": " dd '\n",
            "malformed_adjacent": " dd 'AB''CD'\n",
            "malformed_after": " dd 'AB'garbage\n",
            "quoted_pair": " db 'AB',\"AB\",7\n",
            "backslash_pair": " db '\\n',\"\\n\"\n",
            "abyte": " ABYTE 1,'AB'\n",
            "abytec": " ABYTEC 1,'AB'\n",
        }
    )
    return out


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tool = Path("/usr/local/bin/sjasmplus")
    rows = []
    with tempfile.TemporaryDirectory(prefix="asm198x-packed-single-native-") as td:
        root = Path(td)

        def run(argv: list[str]) -> dict:
            p = subprocess.run(
                argv,
                cwd=root,
                capture_output=True,
                text=True,
                errors="replace",
                timeout=5,
                check=False,
            )
            return {
                "argv": argv,
                "exit": p.returncode,
                "stdout": p.stdout,
                "stderr": p.stderr,
            }

        version = run([str(tool), "--version"])
        assert version["exit"] == 0 and "v1.21.0" in version["stderr"]
        for name, source in cases().items():
            (root / "input.asm").write_text(source)
            (root / "native.bin").unlink(missing_ok=True)
            result = run([str(tool), "--raw=native.bin", "input.asm"])
            rows.append(
                {
                    "name": name,
                    "source": source,
                    "native": result,
                    "native_hex": (root / "native.bin").read_bytes().hex()
                    if (root / "native.bin").exists()
                    else None,
                }
            )
    args.output.write_text(
        json.dumps(
            {
                "reference": {
                    "path": str(tool),
                    "sha256": hashlib.sha256(tool.read_bytes()).hexdigest(),
                    "version": version,
                },
                "cases": rows,
            },
            indent=2,
        )
        + "\n"
    )
    print("cases", len(rows), "accepted", sum(r["native"]["exit"] == 0 for r in rows))


if __name__ == "__main__":
    main()
