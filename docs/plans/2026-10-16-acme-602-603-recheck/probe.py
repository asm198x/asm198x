"""Recheck the two ACME reports against independent native output."""

import hashlib
import json
import platform
import subprocess
import sys
import tempfile
from pathlib import Path

CASES = {
    "602_exact": "!cpu 6502\n* = $1001\n        lda #1\n",
    "602_selected_cpu": "!cpu 65c02\n* = $1001\n        stz $12\n",
    "603_exact": "        * = $1000\n        nop\n!if * > $0fff {\n        nop\n}\n",
    "603_false_else": "*=$1000\nnop\n!if * = $1000 { !byte 1 } else { !byte 2 }\n",
    "603_advances": "*=$1000\n!if * = $1000 { !byte $11 }\n!if * = $1001 { !byte $22 }\n",
    "603_logical_pc": "*=$1000\n!pseudopc $2000 {\nnop\n!if * = $2001 { !byte 11 } else { !byte 12 }\n}\n!if * = $1002 { !byte 13 }\n",
    "603_multiply_prefix": "*=$1000\n!if 2 * >$1234 = $24 { !byte 6 }\n",
}
INVALID = {
    "602_code_without_origin": "!cpu 6502\n lda #1\n",
    "603_condition_without_origin": "!if * > 0 { !byte 1 }\n",
}


def digest(file: Path) -> str:
    return hashlib.sha256(file.read_bytes()).hexdigest()


def run(argv: list[str], cwd: Path) -> dict:
    result = subprocess.run(
        argv, cwd=cwd, capture_output=True, text=True, timeout=30, check=False
    )
    return {
        "argv": argv,
        "cwd": str(cwd),
        "exit": result.returncode,
        "stdout": result.stdout,
        "stderr": result.stderr,
    }


def probe(assembler: Path, native: Path, output: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="asm198x-acme-recheck-") as folder:
        root = Path(folder)
        versions = [run([str(tool), "--version"], root) for tool in [assembler, native]]
        assert all(v["exit"] == 0 for v in versions), versions
        assert "asm198x v0.0.67" in versions[0]["stdout"] + versions[0]["stderr"]
        assert "release 0.97" in versions[1]["stdout"] + versions[1]["stderr"]
        records = []
        for name, source in CASES.items():
            file = root / "input.a"
            file.write_text(source)
            native_hex = {}
            for fmt in ["plain", "cbm"]:
                reference = root / "native.bin"
                reference.unlink(missing_ok=True)
                n = run([str(native), "-f", fmt, "-o", str(reference), str(file)], root)
                assert n["exit"] == 0 and reference.is_file(), n
                expected = reference.read_bytes()
                assert expected, name
                native_hex[fmt] = expected.hex()
                for mode in ["human", "json"]:
                    result_file = root / "result.bin"
                    result_file.write_bytes(b"previous image")
                    flags = ["--prg"] if fmt == "cbm" else []
                    a = run(
                        [
                            str(assembler),
                            "--dialect",
                            "acme",
                            *flags,
                            "--message-format",
                            mode,
                            str(file),
                            "-o",
                            str(result_file),
                        ],
                        root,
                    )
                    assert a["exit"] == 0, a
                    assert result_file.read_bytes() == expected, (name, fmt, a)
                    if mode == "json":
                        assert not a["stderr"]
                        assert not json.loads(a["stdout"])["diagnostics"]
                    assert file.read_text() == source
                    records.append(
                        {
                            "case": name,
                            "source": source,
                            "format": fmt,
                            "mode": mode,
                            "native": n,
                            "asm198x": a,
                            "hex": expected.hex(),
                            "status": "pass",
                        }
                    )
            assert native_hex["cbm"][4:] == native_hex["plain"], name
            formatted = root / "formatted.a"
            f = run(
                [
                    str(assembler),
                    "fmt",
                    "--dialect",
                    "acme",
                    str(file),
                    "-o",
                    str(formatted),
                ],
                root,
            )
            assert f["exit"] == 0 and formatted.is_file(), f
            a = run(
                [
                    str(assembler),
                    "--dialect",
                    "acme",
                    str(formatted),
                    "-o",
                    str(root / "formatted.bin"),
                ],
                root,
            )
            assert (
                a["exit"] == 0
                and (root / "formatted.bin").read_bytes().hex() == native_hex["plain"]
            ), a
            records.append(
                {
                    "case": name,
                    "source": source,
                    "format": "formatted",
                    "formatter": f,
                    "formatted_source": formatted.read_text(),
                    "asm198x": a,
                    "hex": native_hex["plain"],
                    "status": "pass",
                }
            )
        for name, source in INVALID.items():
            file.write_text(source)
            reference = root / "invalid-native.bin"
            n = run([str(native), "-f", "plain", "-o", str(reference), str(file)], root)
            assert n["exit"] != 0 and not reference.exists(), n
            for mode in ["human", "json"]:
                destination = root / "invalid.bin"
                destination.write_bytes(b"previous image")
                a = run(
                    [
                        str(assembler),
                        "--dialect",
                        "acme",
                        "--message-format",
                        mode,
                        str(file),
                        "-o",
                        str(destination),
                    ],
                    root,
                )
                assert (
                    a["exit"] != 0 and destination.read_bytes() == b"previous image"
                ), a
                if mode == "json":
                    assert not a["stderr"] and json.loads(a["stdout"]), a
                assert file.read_text() == source
                records.append(
                    {
                        "case": name,
                        "source": source,
                        "mode": mode,
                        "native": n,
                        "asm198x": a,
                        "previous_hex": b"previous image".hex(),
                        "after_hex": destination.read_bytes().hex(),
                        "status": "pass",
                    }
                )
        assert len(records) == 39
        report = {
            "host": {
                "platform": platform.platform(),
                "machine": platform.machine(),
                "python": platform.python_version(),
            },
            "versions": versions,
            "executables": {str(tool): digest(tool) for tool in [assembler, native]},
            "records": records,
            "qualified": True,
        }
        output.write_text(json.dumps(report, indent=2) + "\n")
        print(
            "Verified 28 native byte comparisons, seven formatter round trips and four rejection/preservation controls"
        )


if __name__ == "__main__":
    probe(
        Path(sys.argv[1]).resolve(),
        Path(sys.argv[2]).resolve(),
        Path(sys.argv[3]).resolve(),
    )
