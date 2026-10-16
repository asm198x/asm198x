"""Measure decoded and literal SjASMPlus backslash pairs separately."""

import hashlib
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

tool, output = sys.argv[1:]
rows = []
for char in map(chr, range(32, 127)):
    suffix = "\\" + char
    source = ' org 8000h\n db "a' + suffix + ';b",7\n'
    with tempfile.TemporaryDirectory(
        dir=os.environ.get("TMPDIR", "/private/tmp")
    ) as temp:
        p = Path(temp)
        (p / "input.asm").write_text(source)
        argv = [tool, "--raw=out.bin", "input.asm"]
        result = subprocess.run(
            argv, cwd=p, capture_output=True, text=True, timeout=30, check=False
        )
        assert result.returncode == 0
        rows.append(
            {
                "escape": suffix,
                "source": source,
                "unsupported": (p / "out.bin").read_bytes()
                != ("a" + suffix + ";b").encode() + bytes([7]),
                "argv": argv,
                "exit": result.returncode,
                "stdout": result.stdout,
                "stderr": result.stderr,
                "hex": (p / "out.bin").read_bytes().hex(),
            }
        )
print(len(rows), [(row["escape"], row["hex"]) for row in rows if row["unsupported"]])
assert len(rows) == 95 and sum(row["unsupported"] for row in rows) == 23
assert {row["escape"][-1] for row in rows if row["unsupported"]} == set(
    "\\\"'0?abdefnrtvABDEFNRTV"
)
directives = []
for base in [
    "db",
    "defb",
    "defm",
    "dm",
    "byte",
    "dw",
    "defw",
    "word",
    "dword",
    "dd",
    "defd",
    "d24",
    "dz",
    "dc",
    "abyte",
    "abytec",
    "abytez",
]:
    for word in [base, "." + base, base.upper()]:
        offset = "0 " if base.startswith("abyte") else ""
        source = " org 8000h\n " + word + " " + offset + '"a' + chr(92) + 'n;b",7\n'
        with tempfile.TemporaryDirectory(
            dir=os.environ.get("TMPDIR", "/private/tmp")
        ) as temp:
            p = Path(temp)
            (p / "input.asm").write_text(source)
            argv = [tool, "--raw=out.bin", "input.asm"]
            result = subprocess.run(
                argv, cwd=p, capture_output=True, text=True, timeout=10, check=False
            )
            assert result.returncode == 0, (word, result.stderr)
            directives.append(
                {
                    "word": word,
                    "source": source,
                    "argv": argv,
                    "exit": result.returncode,
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                    "hex": (p / "out.bin").read_bytes().hex(),
                }
            )
assert len(directives) == 51
version = subprocess.run(
    [tool, "--version"], capture_output=True, text=True, timeout=30, check=False
)
assert "SjASMPlus Z80 Cross-Assembler v1.21.0" in version.stdout + version.stderr
Path(output).write_text(
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
            "directives": directives,
        },
        indent=2,
    )
    + "\n"
)
