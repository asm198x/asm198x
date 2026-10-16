"""Retain fresh native SjASMPlus quoted-comment facts for issue 695 regression fixtures."""

import hashlib
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

root = Path(os.environ.get("TMPDIR", "/private/tmp"))
tool = sys.argv[1]
output = Path(sys.argv[2])
cases = {
    "semicolon_string": ' org 8000h\n db "a;b",7\n',
    "slashes_string": ' org 8000h\n db "a//b",7\n',
    "semicolon_character": " org 8000h\n db ';',7\n",
    "mixed": " org 8000h\n db '\"',\"a';//b\",';',',',7 ; actual comment\n",
    "real_semicolon": ' org 8000h\n db "ok",7 ; db 99, "unterminated\n',
    "real_slashes": ' org 8000h\n db "ok",7 // db 99, "unterminated\n',
    "comment_order": ' org 8000h\n db "a//b;c",7 ; // db 99\n db "d;e//f",8 // ; db 99\n',
    "register": " org 8000h\n ex af,af' ; comment with ' and nonsense\n ex af,af' // comment with ' and nonsense\n db \";\"\n",
    "backslash_character": " org 8000h\n db '\\',7\n",
    "word": " org 8000h\n dw ';',',',7 ; real\n",
    "macro": ' org 8000h\nput macro\n db "a;b//c",7 ; body\n endm\n put\n',
    "repetition": ' org 8000h\n rept 2\n db "a;b//c",7 // body\n endr\n',
    "unterminated": ' org 8000h\n db "a;b//c\n',
    "malformed": ' org 8000h\n db "a;b//c",?\n',
}
rows = []
for name, source in cases.items():
    with tempfile.TemporaryDirectory(dir=root) as temp:
        p = Path(temp)
        (p / "input.asm").write_text(source)
        argv = [tool, "--raw=out.bin", "input.asm"]
        result = subprocess.run(
            argv, cwd=p, capture_output=True, text=True, timeout=30, check=False
        )
        row = {
            "name": name,
            "source": source,
            "argv": argv,
            "exit": result.returncode,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "hex": (p / "out.bin").read_bytes().hex()
            if (p / "out.bin").exists()
            else None,
        }
        rows.append(row)
        print(
            name,
            row["exit"],
            row["hex"],
            repr(result.stderr) if result.returncode else "",
        )
version = subprocess.run(
    [tool, "--version"], capture_output=True, text=True, timeout=30, check=False
)
assert "SjASMPlus Z80 Cross-Assembler v1.21.0" in version.stdout + version.stderr
assert len(rows) == 14 and sum(row["exit"] == 0 for row in rows) == 12
assert rows[0]["hex"] == "613b6207" and rows[1]["hex"] == "612f2f6207"
output.write_text(
    json.dumps(
        {
            "reference": {
                "path": tool,
                "sha256": hashlib.sha256(Path(tool).read_bytes()).hexdigest(),
                "version_process": {
                    "exit": version.returncode,
                    "stdout": version.stdout,
                    "stderr": version.stderr,
                },
            },
            "cases": rows,
        },
        indent=2,
    )
    + "\n"
)
