"""Retain fresh native Z80 character data-list facts for issue 693 regression fixtures."""

import hashlib
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

root = Path(os.environ.get("TMPDIR", "/private/tmp"))
tools = {"pasmo": sys.argv[1], "sjasmplus": sys.argv[2]}
output = Path(sys.argv[3])
cases = {
    "quote": " org 8000h\n db '\"',7\n",
    "comma": " org 8000h\n db ',',7\n",
    "mixed": " org 8000h\n db 1,'\"',\",x\",',',\"a'b,c\",7\n",
    "word": " org 8000h\n dw '\"',',',7\n",
    "expressions": " org 8000h\n db ','+1,'\"'+1,7\n",
    "comment": " org 8000h\n db '\"',',',7 ; db 99, \"unterminated\n",
    "repetition": " org 8000h\n rept 2\n db '\"',',',7\n endm\n",
    "macro": " org 8000h\nput macro v\n db v,7\n endm\n put '\"'\n put ','\n",
    "semicolon": " org 8000h\n db ';','\"',',',7 ; real comment\n",
    "malformed_character": " org 8000h\n db ',,7\n",
    "malformed_value": " org 8000h\n db '\"',?,7\n",
}
rows = []
references = {}
for dialect, tool in tools.items():
    version = subprocess.run(
        [tool] if dialect == "pasmo" else [tool, "--version"],
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )
    assert (
        "PasmoNext v0.1.3"
        if dialect == "pasmo"
        else "SjASMPlus Z80 Cross-Assembler v1.21.0"
    ) in version.stdout + version.stderr
    references[dialect] = {
        "path": tool,
        "sha256": hashlib.sha256(Path(tool).read_bytes()).hexdigest(),
        "version_process": {
            "exit": version.returncode,
            "stdout": version.stdout,
            "stderr": version.stderr,
        },
    }
    for name, original in cases.items():
        # SjASMPlus's semicolon-comment handling is outside this delimiter fix.
        if dialect == "sjasmplus" and name == "semicolon":
            continue
        source = (
            original.replace(" endm\n", " endr\n")
            if dialect == "sjasmplus" and name == "repetition"
            else original
        )
        with tempfile.TemporaryDirectory(dir=root) as temp:
            p = Path(temp)
            (p / "input.asm").write_text(source)
            argv = (
                [tool, "--bin", "input.asm", "out.bin"]
                if dialect == "pasmo"
                else [tool, "--raw=out.bin", "input.asm"]
            )
            result = subprocess.run(
                argv, cwd=p, capture_output=True, text=True, timeout=30, check=False
            )
            row = {
                "dialect": dialect,
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
                dialect,
                name,
                row["exit"],
                row["hex"],
                repr(result.stderr) if result.returncode else "",
            )
assert len(rows) == 21
assert sum(row["exit"] == 0 for row in rows) == 17
assert all(row["hex"] == "2207" for row in rows if row["name"] == "quote")
assert all(row["hex"] == "2c07" for row in rows if row["name"] == "comma")
output.write_text(
    json.dumps({"references": references, "cases": rows}, indent=2) + "\n"
)
