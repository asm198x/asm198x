"""Retain bounded native DISPLAY executions, including text and byte output."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    for mode in ["", "/H,", "/h,", "/D,", "/d,", "/A,", "/B,", "/C,"]:
        for value in [
            "0",
            "1",
            "255",
            "256",
            "$FFFF",
            "$10000",
            "$12345678",
            "-1",
            "-2147483648",
            "$FFFFFFFF",
        ]:
            out[f"mode_{mode or 'default'}_{value}"] = (
                f" DISPLAY {mode}{value}\n db 1\n"
            )
    for i, args in enumerate(
        [
            '"Code start: ",/H,$8000',
            "/D,1,2",
            '/D,"str,;://",10,11',
            "/D,/L,/T,10,11",
            "/A,1,/B,2,/C,65,3",
            "/L,5",
            "/T,5",
            "/D",
            "/H,",
            "",
            '""',
            '"hello"',
            "'ab,cd'",
            '"a\\"b,c"',
            '"a\\tb"',
            '"a\\nB"',
            '"a\\0B"',
            '"£"',
            "/X,1",
            "/,1",
            "/HH,1",
            "/H 1",
            "/ H,1",
            "1,,2",
            "1,",
            '"a"+1',
            '"a",(8/2)',
            "/B,$1234",
            "/C,127",
            "/C,10",
            "/C,39",
            "/C,92",
            "1/0",
            "missing",
            "/D,$",
        ]
    ):
        out[f"args_{i}"] = f" DISPLAY {args}\n db 1\n"
    out.update(
        {
            "forward": " DISPLAY /D,end-start\nstart: db 1,2\nend:\n",
            "forward_division": " DISPLAY /D,10/N\nN equ 2\n db 1\n",
            "forward_division_zero": " DISPLAY /D,10/N\nN equ 0\n db 1\n",
            "temporary_anchor": "1: DISPLAY /D,1\n jr 1B\n",
            "forward_equ": " DISPLAY /D,N\nN equ 37\n db 1\n",
            "local": " org $8000\nmain: db 1\n.local: DISPLAY /H,.local\n",
            "pc": " org $8000\n db 1\n DISPLAY /H,$\n",
            "inactive": " IF 0\n DISPLAY /X,missing\n ENDIF\n db 1\n",
            "macro": " MACRO show value\n DISPLAY /D,value\n ENDM\n show 42\n db 1\n",
            "reset_line": " DISPLAY /D,10\n DISPLAY 10\n db 1\n",
            "module_fallback": "N equ 37\n MODULE inner\n DISPLAY /D,N\n ENDMODULE\n db 1\n",
            "module_forward": " MODULE inner\n DISPLAY /D,N\nN equ 37\n ENDMODULE\n db 1\n",
            "local_forward": "main: DISPLAY /H,.end\n db 1\n.end:\n",
            "quoted_single_pair": " DISPLAY 'a''b'\n db 1\n",
            "quoted_double_pair": ' DISPLAY "a""b"\n db 1\n',
            "single_backslash": " DISPLAY 'a\\nb'\n db 1\n",
            "quoted_expression": " DISPLAY /D,('A'+1)\n db 1\n",
        }
    )
    return out


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tool = Path("/usr/local/bin/sjasmplus")
    records = []
    with tempfile.TemporaryDirectory(prefix="asm198x-display-format-probe-") as temp:
        root = Path(temp)

        def run(argv: list[str]) -> dict:
            r = subprocess.run(
                argv, cwd=root, capture_output=True, timeout=10, check=False
            )
            return {
                "exit": r.returncode,
                "stdout": r.stdout.decode("utf-8", errors="replace"),
                "stderr": r.stderr.decode("utf-8", errors="replace"),
            }

        version = run([str(tool), "--version"])
        for name, source in cases().items():
            (root / "input.asm").write_text(source)
            (root / "native.bin").unlink(missing_ok=True)
            record = run(
                [
                    str(tool),
                    "--nologo",
                    "--color=off",
                    "--msg=err",
                    "--raw=native.bin",
                    "input.asm",
                ]
            )
            records.append(
                {
                    "name": name,
                    "source": source,
                    **record,
                    "hex": (root / "native.bin").read_bytes().hex()
                    if (root / "native.bin").exists()
                    else None,
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
    for r in records:
        print(r["name"], r["exit"], repr(r["stderr"]))


if __name__ == "__main__":
    main()
