#!/usr/bin/env python3
"""Record bounded workflow invariants; violations are findings, not passing tests."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import re
import subprocess
import tempfile
from pathlib import Path
from typing import Any


def audit(binary: Path) -> dict[str, Any]:
    results: list[dict[str, Any]] = []

    def record(
        name: str,
        checks: dict[str, bool],
        runs: list[dict[str, Any]],
        details: dict[str, Any],
        *,
        control: bool = False,
    ) -> None:
        assert checks, name
        if control and not all(checks.values()):
            raise AssertionError(f"control failed: {name}: {checks}")
        results.append(
            {
                "name": name,
                "control": control,
                "checks": checks,
                "violations": [k for k, v in checks.items() if not v],
                "runs": runs,
                "details": details,
            }
        )

    with tempfile.TemporaryDirectory(prefix="asm198x-formatter-audit-") as td:
        root = Path(td)
        counter = 0

        def fixture(source: str) -> Path:
            nonlocal counter
            counter += 1
            directory = root / str(counter)
            directory.mkdir()
            (directory / "program.s").write_text(source)
            return directory

        def run(directory: Path, args: list[str]) -> dict[str, Any]:
            p = subprocess.run(
                [str(binary), *args],
                cwd=directory,
                capture_output=True,
                text=True,
                timeout=15,
                check=False,
            )
            return {
                "args": args,
                "exit": p.returncode,
                "stdout": p.stdout,
                "stderr": p.stderr,
            }

        samples = {
            "acme-conditional": (
                "acme",
                "*=$1000\n!if 1 { ; head\n!byte 1\n} else { ; middle\n!byte 2\n} ; tail\n",
            ),
            "acme-for": ("acme", "*=$1000\n!for i,1,3 { ; head\n!byte i\n} ; tail\n"),
            "acme-do": (
                "acme",
                "*=$1000\n!set i=0\n!do { ; head\n!byte i\n!set i=i+1\n} until i=3 ; tail\n",
            ),
            "sjasmplus-dup": (
                "sjasmplus",
                " org $8000\n DUP 2 ; head\n DB 1\n EDUP ; tail\n",
            ),
            "acme-zone-control": (
                "acme",
                "*=$1000\n!zone z { ; head\n.loop: nop\n} ; tail\n",
            ),
            "acme-macro-control": (
                "acme",
                "*=$1000\n!macro m .a { ; head\nlda #.a\n} ; tail\n+m 2\n",
            ),
            "sjasmplus-if-control": (
                "sjasmplus",
                " org $8000\n IF 1 ; head\n DB 1\n ELSE ; middle\n DB 2\n ENDIF ; tail\n",
            ),
        }
        for name, (dialect, source) in samples.items():
            d = fixture(source)
            args = ["--dialect", dialect]
            runs = [
                run(d, [*args, "program.s", "-o", "original.bin"]),
                run(d, ["fmt", *args, "program.s", "-o", "formatted.s"]),
            ]
            assert all(r["exit"] == 0 for r in runs), runs
            formatted = (d / "formatted.s").read_text()
            runs += [
                run(d, [*args, "formatted.s", "-o", "formatted.bin"]),
                run(d, ["fmt", *args, "formatted.s", "-o", "twice.s"]),
            ]
            assert all(r["exit"] == 0 for r in runs), runs
            missing = [
                c
                for c in ["; head", "; middle", "; tail"]
                if c in source and c not in formatted
            ]
            record(
                name,
                {
                    "comments_preserved": not missing,
                    "byte_parity": (d / "original.bin").read_bytes()
                    == (d / "formatted.bin").read_bytes(),
                    "idempotent": formatted == (d / "twice.s").read_text(),
                },
                runs,
                {"source": source, "formatted": formatted, "missing_comments": missing},
                control=name.endswith("control"),
            )

        for mode in ["human", "json"]:
            for count in [0, 1, 2]:
                definitions = [
                    v for n in range(count) for v in ["--equ", f"FEATURE{n}=1"]
                ]
                source = " org $8000\n ld a,(\n"
                d = fixture(source)
                (d / "image.bin").write_bytes(b"previous image")
                r = run(
                    d,
                    [
                        "--dialect",
                        "pasmo",
                        "--message-format",
                        mode,
                        "program.s",
                        "-o",
                        "image.bin",
                        *definitions,
                    ],
                )
                if mode == "json":
                    payload = json.loads(r["stdout"])
                    assert len(payload) == 1 and not r["stderr"], r
                    actual = payload[0]["span"]["line"]
                else:
                    match = re.search(r"program.s:(\d+):", r["stderr"])
                    assert match, r
                    actual = int(match[1])
                record(
                    f"equ-error-{mode}-{count}",
                    {
                        "rejected": r["exit"] != 0,
                        "root_line_is_two": actual == 2,
                        "image_preserved": (d / "image.bin").read_bytes()
                        == b"previous image",
                    },
                    [r],
                    {"source": source, "reported_line": actual},
                    control=count == 0,
                )

        for count in [0, 1, 2]:
            definitions = [v for n in range(count) for v in ["--equ", f"FEATURE{n}=1"]]
            source = " org $8000\nstart: ld a,1\n"
            d = fixture(source)
            r = run(
                d,
                [
                    "--dialect",
                    "pasmo",
                    "--message-format=json",
                    "program.s",
                    *definitions,
                ],
            )
            assert r["exit"] == 0 and not r["stderr"], r
            payload = json.loads(r["stdout"])
            lines = [line["line"] for line in payload["debug"]["lines"]]
            cycles = [line["line"] for line in payload["debug"]["cycles"]]
            record(
                f"equ-debug-{count}",
                {
                    "machine_code": payload["bytes"] == [62, 1],
                    "debug_root_line_is_two": lines == [2],
                    "cycle_root_line_is_two": cycles == [2],
                },
                [r],
                {"source": source, "debug_lines": lines, "cycle_lines": cycles},
                control=count == 0,
            )

        for dialect, flag, source in [
            ("acme", "--prg", "*=$1000\nlda #1\n"),
            ("pasmo", "--tapbas", " org $8000\nstart: nop\n end start\n"),
            ("vasm", "--exe", "start: moveq #1,d0\n rts\n"),
            (
                "rgbasm",
                "--gb-rom",
                'SECTION "main", ROM0[$100]\n ds $50,0\nstart: nop\n',
            ),
        ]:
            d = fixture(source)
            # Both operations work separately: this tests the incompatible combination.
            asm = run(d, ["--dialect", dialect, "program.s", flag, "-o", "image.bin"])
            fmt = run(d, ["fmt", "--dialect", dialect, "program.s", "-o", "valid.s"])
            assert asm["exit"] == fmt["exit"] == 0, [asm, fmt]
            (d / "destination").write_bytes(b"previous destination")
            r = run(
                d, ["fmt", "--dialect", dialect, "program.s", flag, "-o", "destination"]
            )
            record(
                f"fmt-container-{flag}",
                {
                    "incompatible_option_rejected": r["exit"] != 0,
                    "destination_preserved": (d / "destination").read_bytes()
                    == b"previous destination",
                },
                [asm, fmt, r],
                {"source": source, "destination": (d / "destination").read_text()},
            )

        for flag in ["--sym", "--equ=FEATURE=1"]:
            source = " org $8000\n ld a,1\n"
            d = fixture(source)
            (d / "destination").write_bytes(b"previous destination")
            r = run(
                d, ["fmt", "--dialect", "pasmo", "program.s", flag, "-o", "destination"]
            )
            record(
                f"fmt-rejection-control-{flag}",
                {
                    "rejected": r["exit"] != 0,
                    "destination_preserved": (d / "destination").read_bytes()
                    == b"previous destination",
                },
                [r],
                {"source": source},
                control=True,
            )

        d = fixture("*=$1000\n!if 1 {\n!byte 1\n")
        (d / "destination").write_bytes(b"previous destination")
        r = run(d, ["fmt", "--dialect", "acme", "program.s", "-o", "destination"])
        record(
            "fmt-invalid-source-control",
            {
                "rejected": r["exit"] != 0,
                "destination_preserved": (d / "destination").read_bytes()
                == b"previous destination",
            },
            [r],
            {},
            control=True,
        )

    assert len(results) == 23, len(results)
    return {
        "identity": {
            "source_commit": subprocess.check_output(
                ["git", "rev-parse", "HEAD"], text=True
            ).strip(),
            "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "binary_version": subprocess.run(
                [str(binary), "--version"], capture_output=True, text=True, check=True
            ).stderr.strip(),
            "host": platform.platform(),
            "python": platform.python_version(),
        },
        "cases": results,
        "summary": {
            "cases": len(results),
            "violating_cases": sum(bool(r["violations"]) for r in results),
            "passing_cases": sum(not r["violations"] for r in results),
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = audit(args.binary.resolve(strict=True))
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result["summary"]))


if __name__ == "__main__":
    main()
