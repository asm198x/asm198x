"""Retain assembly and unexpanded formatter outcomes from an actual binary."""

import argparse
import hashlib
import json
import re
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).parent


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    facts = json.loads((ROOT / "native.json").read_text())
    rows = []
    with tempfile.TemporaryDirectory(prefix="asm198x-huc-macro-format-") as temp:
        root = Path(temp)
        for fact in facts["cases"]:
            (root / "input.s").write_text(fact["source"])
            dialect = fact["name"].split("_", 1)[0]
            runs = []

            def run(
                argv: list[str], records: list[dict] = runs
            ) -> subprocess.CompletedProcess[str]:
                command = [str(binary), *argv]
                result = subprocess.run(
                    command,
                    cwd=root,
                    capture_output=True,
                    text=True,
                    timeout=5,
                    check=False,
                )
                records.append(
                    {
                        "argv": command,
                        "exit": result.returncode,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                    }
                )
                return result

            first = run(["--dialect", dialect, "input.s", "-o", "out.bin"])
            native_match = (
                first.returncode == 0
                and (root / "out.bin").read_bytes().hex() == fact["native_hex"]
            )
            formatted = run(["fmt", "--dialect", dialect, "input.s"])
            roundtrip = False
            if formatted.returncode == 0:
                (root / "formatted.s").write_text(formatted.stdout)
                again = run(["fmt", "--dialect", dialect, "formatted.s"])
                replay = run(["--dialect", dialect, "formatted.s", "-o", "out.bin"])
                definitions = r"(?mi)^\s*\.(?:macro|mac)\s"
                roundtrip = (
                    again.returncode == 0
                    and again.stdout == formatted.stdout
                    and replay.returncode == 0
                    and (root / "out.bin").read_bytes().hex() == fact["native_hex"]
                    and len(re.findall(definitions, formatted.stdout))
                    == len(re.findall(definitions, fact["source"]))
                )
            rows.append(
                {
                    "name": fact["name"],
                    "matches_native": native_match,
                    "formatter_roundtrip": roundtrip,
                    "runs": runs,
                }
            )
    assert len(rows) == 32
    args.output.write_text(
        json.dumps(
            {
                "binary": str(binary),
                "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "results": rows,
            },
            indent=2,
        )
        + "\n"
    )
    print(
        "cases",
        len(rows),
        "assembly differences",
        sum(not r["matches_native"] for r in rows),
        "formatter differences",
        sum(not r["formatter_roundtrip"] for r in rows),
    )


if __name__ == "__main__":
    main()
