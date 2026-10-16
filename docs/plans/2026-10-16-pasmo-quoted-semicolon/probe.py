"""Retain fresh native Pasmo quote/comment facts for issue 690 regression fixtures."""

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
    "double": ' org 8000h\n db "rept;endm"\n',
    "character": " org 8000h\n db ';' ; actual comment\n",
    "mixed": " org 8000h\n db \"a;b,c\", ';', 42 ; db 99\n db \"a'b;c\", 'x', 7\n",
    "register": " org 8000h\n ex af,af' ; comment with ' and nonsense\n db \";\" ; final\n",
    "slashes": ' org 8000h\n db "a//b;c" ; real\n',
    "macro": ' org 8000h\nsetv macro\n rept 2\n db "rept;endm" ; body\n endm\n endm\n setv\n',
    "unterminated": ' org 8000h\n db "rept;endm\n',
    "trailing": ' org 8000h\n db "ok" ; "unterminated\n',
}
rows = []
for name, source in cases.items():
    with tempfile.TemporaryDirectory(dir=root) as t:
        p = Path(t)
        (p / "input.asm").write_text(source)
        r = subprocess.run(
            [tool, "--bin", "input.asm", "out.bin"],
            cwd=p,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
        row = {
            "name": name,
            "source": source,
            "argv": ["pasmo", "--bin", "input.asm", "out.bin"],
            "exit": r.returncode,
            "stdout": r.stdout,
            "stderr": r.stderr,
            "hex": (p / "out.bin").read_bytes().hex()
            if (p / "out.bin").exists()
            else None,
        }
        rows.append(row)
        print(name, r.returncode, row["hex"], repr(r.stderr))
version = subprocess.run(
    [tool], capture_output=True, text=True, timeout=30, check=False
)
assert "PasmoNext v0.1.3" in version.stderr
assert len(rows) == 8 and sum(row["exit"] == 0 for row in rows) == 7
assert rows[0]["hex"] == "726570743b656e646d"

data = {
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
}
output.write_text(json.dumps(data, indent=2) + "\n")
