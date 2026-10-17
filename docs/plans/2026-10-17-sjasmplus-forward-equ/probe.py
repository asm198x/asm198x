"""Retain bounded native forward EQU executions and exact output bytes."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path


def cases() -> dict[str, str]:
    out = {
        "original": "TOTAL EQU MAX_PLAYERS*3\n db TOTAL\nMAX_PLAYERS EQU 2\n",
        "ordered": "MAX_PLAYERS EQU 2\nTOTAL EQU MAX_PLAYERS*3\n db TOTAL\n",
        "direct": " db MAX_PLAYERS\nMAX_PLAYERS EQU 2\n",
        "missing": "TOTAL EQU missing*3\n db TOTAL\n",
        "self_cycle": "TOTAL EQU TOTAL+1\n dd TOTAL\n",
        "pair_cycle": "A EQU B+1\nB EQU A+1\n dd A,B\n",
        "division": "TOTAL EQU 12/N\n db TOTAL\nN EQU 3\n",
        "division_zero": "TOTAL EQU 12/N\n db TOTAL\nN EQU 0\n",
        "pc": " org $8000\nTOTAL EQU later-$\n db 1,2,3\nlater: dw TOTAL\n",
        "forward_address": "TOTAL EQU later+1\n dw TOTAL\nlater: db 7\n",
        "local": "main: \n.total EQU .count*3\n db .total\n.count EQU 2\n",
        "module": " MODULE inner\nTOTAL EQU COUNT*3\n db TOTAL\nCOUNT EQU 2\n ENDMODULE\n",
        "module_fallback": " MODULE inner\nTOTAL EQU COUNT*3\n db TOTAL\n ENDMODULE\nCOUNT EQU 2\n",
        "module_shadow": "COUNT EQU 5\n MODULE inner\nTOTAL EQU COUNT*3\n db TOTAL\nCOUNT EQU 2\n ENDMODULE\n",
        "conditional": "TOTAL EQU COUNT*3\n IF TOTAL=6\n db 9\n ENDIF\nCOUNT EQU 2\n",
        "reserve": "TOTAL EQU COUNT*3\n ds TOTAL,7\nCOUNT EQU 2\n",
        "inactive": " IF 0\nTOTAL EQU missing*3\n ENDIF\n db 7\n",
        "duplicate": "TOTAL EQU COUNT*3\nTOTAL EQU 4\n db TOTAL\nCOUNT EQU 2\n",
    }
    out.update(
        {
            "pc_backward": " org $8000\nstart: db 1,2\nTOTAL EQU $-start\n dw TOTAL\n",
            "nested_module": " MODULE outer\n MODULE inner\nTOTAL EQU COUNT*3\n db TOTAL\nCOUNT EQU 2\n ENDMODULE\n ENDMODULE\n",
            "macro": " MACRO derive name,value\nname EQU value*3\n ENDM\n derive TOTAL,COUNT\n db TOTAL\nCOUNT EQU 2\n",
            "local_pc": " org $8000\nmain: \n.total EQU .end-$\n db 1,2\n.end: dw .total\n",
            "unused_missing": "TOTAL EQU missing*3\n db 7\n",
            "unused_division_zero": "TOTAL EQU 12/N\n db 7\nN EQU 0\n",
            "org": "TOTAL EQU COUNT*3\n org TOTAL\n dw $\nCOUNT EQU 2\n",
            "display": "TOTAL EQU COUNT*3\n DISPLAY /D,TOTAL\n db TOTAL\nCOUNT EQU 2\n",
            "equ_case": "TOTAL equ COUNT*3\n db TOTAL\nCOUNT equ 2\n",
            "forward_word": " dw TOTAL\nTOTAL EQU COUNT*3\nCOUNT EQU 2\n",
        }
    )
    out.update(
        {
            "struct_pc": " STRUCT Pair\na DB 1\nb DW 2\n ENDS\n org $8000\nx Pair\nTOTAL EQU $-2\n dw TOTAL\n",
            "struct_label": " STRUCT Pair\na DB 1\nb DW 2\n ENDS\n org $8000\nx Pair\nTOTAL EQU x.b-x.a\n dw TOTAL\n",
            "struct_forward_pc": " STRUCT Pair\na DB 1\nb DW 2\n ENDS\n org $8000\nx Pair\nTOTAL EQU later-$\n db 9\nlater: dw TOTAL\n",
            "struct_repeat_pc": " STRUCT Pair\na DB 1\nb DW 2\n ENDS\n org $8000\n DUP 3\n Pair\n EDUP\nTOTAL EQU $\n dw TOTAL\n",
            "incbin_pc": ' org $8000\n incbin "payload.bin"\nTOTAL EQU $-2\n dw TOTAL\n',
            "incbin_label": ' org $8000\nx incbin "payload.bin"\ny:\nTOTAL EQU y-x\n dw TOTAL\n',
            "include_forward_equ": 'TOTAL EQU COUNT*3\n db TOTAL\n include "values.inc"\n',
            "include_after_struct": ' STRUCT Pair\na DB 1\nb DW 2\n ENDS\n org $8000\nx Pair\n include "values.inc"\nTOTAL EQU $\n dw TOTAL\n',
        }
    )
    for depth in range(1, 7):
        out[f"chain_{depth}"] = (
            "".join(f"C{i} EQU C{i + 1}+1\n" for i in range(depth))
            + f"C{depth} EQU 2\n dd C0\n"
        )
    for expr in [
        "N+1",
        "N-1",
        "N*3",
        "N/3",
        "N%3",
        "N<<4",
        "N>>4",
        "~N",
        "-N",
        "N+2147483647",
        "N*$7FFFFFFF",
    ]:
        for value in [0, 2, 255, 2147483647]:
            out[f"arithmetic_{expr}_{value}"] = (
                f"TOTAL EQU {expr}\n dd TOTAL\nN EQU {value}\n"
            )
    return out


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tool = Path("/usr/local/bin/sjasmplus")
    records = []
    binary_files = {"payload.bin": "0102030405"}
    text_files = {"values.inc": "COUNT EQU 2\n db 8\n"}
    with tempfile.TemporaryDirectory(prefix="asm198x-forward-equ-probe-") as temp:
        root = Path(temp)
        for name, data in binary_files.items():
            (root / name).write_bytes(bytes.fromhex(data))
        for name, text in text_files.items():
            (root / name).write_text(text)

        def run(argv: list[str]) -> dict:
            r = subprocess.run(
                argv, cwd=root, capture_output=True, timeout=10, check=False
            )
            return {
                "argv": argv,
                "cwd": str(root),
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
                "binary_files": binary_files,
                "text_files": text_files,
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
