"""Capture SjASMPlus numeric separators and quote boundaries."""

import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path


def cases() -> dict[str, str]:
    literals = [
        "1'2",
        "$F'F",
        "0xF'F",
        "0XF'F",
        "#F'F",
        "0F'Fh",
        "0F'FH",
        "%1'0",
        "0b1'0",
        "0B1'0",
        "1'0b",
        "1'0B",
        "4'294'967'295",
        "$FF'FF'FF'FF",
        "0'0",
        "000'012",
    ]
    bad = [
        "1'",
        "1''2",
        "$'FF",
        "0x'FF",
        "%'11",
        "#'FF",
        "10'h",
        "10'b",
        "1'_2",
        "1' 2",
        "$F''F",
        "$FF'",
        "0xFF'",
        "%11'",
        "0'xff",
        "0'b10",
        "%1'2",
        "0xF'G",
        "4'294'967'296",
        "$1'0000'0000",
        "1'0junk",
    ]
    out = {}
    for index, literal in enumerate(literals + bad):
        out[f"literal_{index}"] = (
            f"Value equ ({literal})\n dd ({literal}),Value\n IF ({literal})<0\n db $a5\n ELSE\n db $5a\n ENDIF\n ASSERT Value==({literal})\n"
        )
    for index, op in enumerate(
        ["+", "-", "*", "/", "&", "|", "^", "<<", ">>", "==", "&&", "||"]
    ):
        out[f"operator_{index}"] = f" dd (1'0{op}1'1)\n"
    out.update(
        {
            "original": " or %111'00'00'0\n and %111'00'11'1\n",
            "data": " db 1'2,3,';',4,',',6\n dw 1'2,3\n dd 1'2,3\n",
            "colon": " db 1'2 : db 3 ; tail\n db 1'2 : db 4 // tail\n",
            "strings": ' db "1\'2;,//",1\'2,"z"\n ex af,af\'\n',
            "define": " DEFINE FF 7\n DEFINE V 1'2\n dd $F'F,V,FF\n",
            "struct": " STRUCT S\na DB 0\nb DW 0\n ENDS\nthing S {1'2,3}\n",
            "struct_multiline": " STRUCT S\na DB 0\nb DW 0\n ENDS\nthing S {\n 1'2,\n 3}\n",
            "quoted": " db '1',1'2,';',\"1'2\"\n",
            "macro": " MACRO emit x,y\n db x,y\n ENDM\n emit 1'2,3\n",
            "macro_body": " MACRO emit x\n db 1'2,x\n ENDM\n emit 3\n",
            "define_after": " DEFINE X 3\n db 1'2,X\n",
            "macro_hex": " MACRO emit FF\n dd $F'FF\n ENDM\n emit 7\n",
            "macro_hex_first": " MACRO emit F\n dd $F'F\n ENDM\n emit 7\n",
            "macro_comment": " MACRO emit x,y\n db x,y\n ENDM\n emit 1'2,3 ; tail\n",
            "macro_quotes": " MACRO emit x\n db 1'2,x,';',\"1'2\"\n ENDM\n emit 3\n",
            "define_hex": " DEFINE FF 7\n dd $F'FF\n",
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
        prefix="asm198x-digit-separators-native-"
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
