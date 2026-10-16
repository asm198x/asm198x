"""Capture bounded PasmoNext expression facts; no Asm198x expectations inferred."""

import hashlib
import json
import platform
import subprocess
import sys
import tempfile
from pathlib import Path


def capture(tool: Path, output: Path) -> None:
    expressions = [
        "65536/2",
        "(65535+2)/2",
        "-65537/2",
        "(-65535-2)/2",
        "65536",
        "65537",
        "-1",
        "-65536",
        "-65537",
        "65535+2",
        "(65535+2)=1",
        "65536=0",
        "65536<1",
        "-1<1",
        "(-1)<1",
        "-1/2",
        "(-1)/2",
        "-65535-2",
        "(-65535)-2",
        "-(65535+2)",
        "NOT 65536",
        "!65536",
        "65536 && 1",
        "65536 || 0",
        "1/65536",
        "65536/0",
        "65535*65535",
        "(65535*65535)/2",
        "65535 SHL 1",
        "65535 SHR 1",
        "(-1) SHR 1",
        "1 SHL 16",
        "1 SHL 17",
        "65535 SHR 16",
        "1 SHL 65536",
        "1 SHL -1",
        "1 SHL 31",
        "1 SHL 32",
        "1 SHL 63",
        "1 SHL 64",
        "1 SHL 65535",
        "65535 SHR 32",
        "65535 SHR 64",
        "1+-1",
        "1=-1",
        "1 AND -1",
        "1 OR -1",
        "1 XOR NOT 0",
        "1 AND NOT 0",
        "1+( -1)",
        "1+NOT 0",
        "0 && (1/0)",
        "1 || (1/0)",
        "0 && missing",
        "1 || missing",
        "!!65536",
        "1 shl 32",
        "1 ShR 32",
        "1<<32",
        "65535>>32",
        "1/(-1)",
        "1/(65536)",
        "-(-1)",
        "0 && (65535+2)",
        "65536+1",
        "65536-1",
        "65537*2",
        "65537 AND 3",
        "65537 XOR 3",
        "65537 OR 3",
        "$+65536",
        "($+65536)/2",
        "(65535+2) > 0",
    ]
    for left in ["0", "1", "32768", "65535", "65536", "65537", "(-1)", "(-65537)"]:
        for op in ["+", "-", "*", "/", "=", "<", ">", "SHL", "SHR", "AND", "XOR", "OR"]:
            expressions.append(f"{left} {op} 2")
    cases = [
        (f"dw_{i}", f" org $1000\n dw {e}\n", e)
        for i, e in enumerate(dict.fromkeys(expressions))
    ]
    for i, e in enumerate(
        [
            "65536",
            "65537",
            "-1",
            "-65536",
            "-65537",
            "65535+2",
            "65536/2",
            "(-65535-2)/2",
        ]
    ):
        cases.append((f"equ_{i}", f" org $1000\nV equ {e}\n dw V,V/2\n", e))
    cases.extend(
        [
            ("equ_usage", " org $1000\nV equ -1\n dw V,V/2,V<1\n", "V"),
            ("forward", " org $1000\n dw (later+65536)/2\nlater: nop\n", "later"),
            ("backward", " org $1000\nlabel: nop\n dw (label+65536)/2\n", "label"),
            (
                "condition",
                " org $1000\n IF 65536=0\n db $a5\n ELSE\n db $5a\n ENDIF\n",
                "65536=0",
            ),
            (
                "repeat",
                " org $1000\n REPT 65536/2\n db $a5\n ENDM\n db $5a\n",
                "65536/2",
            ),
            ("defs", " org $1000\n defs 65536/2,$a5\n db $5a\n", "65536/2"),
            (
                "immediate",
                " org $1000\n ld hl,65536/2\n ld a,0+(65535+2)/2\n",
                "65536/2",
            ),
        ]
    )
    for i, value in enumerate(
        ["-129", "-128", "-1", "127", "128", "255", "256", "65535", "65536", "32768"]
    ):
        displacement = value if value.startswith("-") else "+" + value
        cases.append((f"index_{i}", f" org $1000\n ld a,(ix{displacement})\n", value))
    with tempfile.TemporaryDirectory(prefix="pasmo-expression-native-") as folder:
        root = Path(folder)
        version = subprocess.run(
            [str(tool)], capture_output=True, text=True, check=False, timeout=30
        )
        records = []
        for name, source, expression in cases:
            (root / "input.asm").write_text(source)
            for file in ["output.bin", "output.sym"]:
                (root / file).unlink(missing_ok=True)
            argv = [str(tool), "--bin", "input.asm", "output.bin", "output.sym"]
            result = subprocess.run(
                argv, cwd=root, capture_output=True, text=True, check=False, timeout=30
            )
            image = root / "output.bin"
            symbols = root / "output.sym"
            data = image.read_bytes() if image.exists() else None
            assert data is None or len(data) <= 65536
            records.append(
                {
                    "name": name,
                    "expression": expression,
                    "source": source,
                    "argv": argv,
                    "exit": result.returncode,
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                    "hex": data.hex() if data is not None else None,
                    "symbols": symbols.read_text() if symbols.exists() else None,
                }
            )
        report = {
            "host": platform.platform(),
            "python": platform.python_version(),
            "tool": str(tool),
            "sha256": hashlib.sha256(tool.read_bytes()).hexdigest(),
            "version": {
                "exit": version.returncode,
                "stdout": version.stdout,
                "stderr": version.stderr,
            },
            "cases": records,
        }
        output.write_text(json.dumps(report, indent=2) + "\n")
        print(
            f"Captured {len(records)} native facts: {sum(r['exit'] == 0 for r in records)} accepted"
        )


if __name__ == "__main__":
    capture(Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve())
