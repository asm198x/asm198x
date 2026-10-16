"""Retain fresh native Pasmo nesting facts for issue 687 regression fixtures."""

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
    "one_level": " org 8000h\nsetv macro\n rept 2\n nop\n endm\n endm\n setv\n",
    "two_levels": " org 8000h\nsetv macro\n rept 2\n rept 3\n nop\n endm\n endm\n endm\n setv\n ret\n",
    "parameters": " org 8000h\n MACRO setv, count\n rept count\n db count\n endm\n endm\n setv 2\n setv 3\n",
    "comments_strings": ' org 8000h\nsetv macro\n REPT 2 ; ENDM is only comment\n db "rept endm"\n ENDM ; inner\n ENDM ; macro\n setv\n',
    "zero_count": " org 8000h\nsetv macro\n rept 0\n nop\n endm\n endm\n setv\n ret\n",
    "labelled_repeat": " org 8000h\nsetv macro\nstart: rept 2\n nop\n endm\n endm\n setv\n",
    "conditional": " org 8000h\nsetv macro\n if 1\n rept 2\n db 5\n endm\n endif\n endm\n setv\n",
    "missing_macro_end": " org 8000h\nsetv macro\n rept 2\n nop\n endm\n",
    "extra_end": " org 8000h\nsetv macro\n rept 2\n nop\n endm\n endm\n endm\n setv\n",
    "body_error": " org 8000h\nsetv macro\n rept 2\n ld a,(\n endm\n endm\n setv\n",
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
assert len(rows) == 10 and sum(row["exit"] == 0 for row in rows) == 6
assert (
    rows[0]["hex"] == "0000"
    and rows[1]["hex"] == "000000000000c9"
    and rows[2]["hex"] == "0202030303"
)
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
