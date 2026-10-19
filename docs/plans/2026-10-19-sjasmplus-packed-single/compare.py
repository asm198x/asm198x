"""Compare a hashed CLI with retained native byte/refusal outcomes."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    facts = json.loads((Path(__file__).parent / "native.json").read_text())
    rows = []
    with tempfile.TemporaryDirectory(prefix="asm198x-packed-single-compare-") as td:
        root = Path(td)
        for fact in facts["cases"]:
            (root / "input.asm").write_text(fact["source"])
            (root / "out.bin").write_bytes(b"previous image")
            argv = [
                str(binary),
                "--dialect",
                "sjasmplus",
                "input.asm",
                "-o",
                "out.bin",
                "--debug=out.debug198x",
            ]
            p = subprocess.run(
                argv, cwd=root, capture_output=True, text=True, timeout=5, check=False
            )
            image = (root / "out.bin").read_bytes()
            accepted = fact["native"]["exit"] == 0
            matches = p.returncode == (0 if accepted else 1) and (
                image.hex() == fact["native_hex"]
                if accepted
                else image == b"previous image"
            )
            rows.append(
                {
                    "name": fact["name"],
                    "argv": argv,
                    "exit": p.returncode,
                    "stdout": p.stdout,
                    "stderr": p.stderr,
                    "hex": image.hex(),
                    "matches_native": matches,
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
    print(
        "cases",
        len(rows),
        "differences",
        sum(not row["matches_native"] for row in rows),
    )


if __name__ == "__main__":
    main()
