"""Compare an explicit executable against the completed native fixture."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("fixture", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    facts = json.loads(args.fixture.read_text())
    rows = []
    with tempfile.TemporaryDirectory(prefix="asm198x-alias-compare-") as td:
        root = Path(td)
        for fact in facts["cases"]:
            assert fact["native"][-1]["exit"] != 124
            (root / "input.s").write_text(fact["source"])
            (root / "input.cfg").write_text(fact["config"])
            (root / "out.bin").unlink(missing_ok=True)
            argv = [
                str(binary),
                "--dialect",
                "ca65",
                "-C",
                "input.cfg",
                "input.s",
                "-o",
                "out.bin",
            ]
            result = subprocess.run(
                argv, cwd=root, capture_output=True, text=True, timeout=5, check=False
            )
            data = (
                (root / "out.bin").read_bytes().hex()
                if (root / "out.bin").exists()
                else None
            )
            matches = (result.returncode == 0) == (
                fact["native"][-1]["exit"] == 0
            ) and (result.returncode != 0 or data == fact["native_hex"])
            rows.append(
                {
                    "name": fact["name"],
                    "argv": argv,
                    "exit": result.returncode,
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                    "hex": data,
                    "native_hex": fact["native_hex"],
                    "matches": matches,
                }
            )
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
    for row in rows:
        if not row["matches"]:
            print(row)
    print(
        len(rows),
        "comparisons;",
        sum(not row["matches"] for row in rows),
        "disagreements",
    )


if __name__ == "__main__":
    main()
