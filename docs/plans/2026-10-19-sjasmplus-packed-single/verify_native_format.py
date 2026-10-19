"""Check canonical output against the retained native assembler byte outcomes."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path


def run(argv: list[str], root: Path) -> dict:
    p = subprocess.run(argv, cwd=root, capture_output=True, timeout=5, check=False)
    return {
        "argv": argv,
        "exit": p.returncode,
        "stdout": p.stdout.decode(),
        "stderr": p.stderr.decode(),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    facts = json.loads((Path(__file__).parent / "native.json").read_text())
    native = Path(facts["reference"]["path"])
    assert (
        hashlib.sha256(native.read_bytes()).hexdigest() == facts["reference"]["sha256"]
    )
    rows = []
    with tempfile.TemporaryDirectory(prefix="asm198x-packed-native-format-") as td:
        root = Path(td)
        for fact in facts["cases"]:
            if fact["native"]["exit"] != 0:
                continue
            (root / "input.asm").write_text(fact["source"])
            formatted = run(
                [str(binary), "fmt", "--dialect", "sjasmplus", "input.asm"], root
            )
            assert formatted["exit"] == 0, (fact["name"], formatted)
            (root / "formatted.asm").write_text(formatted["stdout"])
            (root / "native.bin").unlink(missing_ok=True)
            assembled = run([str(native), "--raw=native.bin", "formatted.asm"], root)
            image = (root / "native.bin").read_bytes()
            assert assembled["exit"] == 0, (fact["name"], assembled)
            assert image.hex() == fact["native_hex"], fact["name"]
            rows.append(
                {
                    "name": fact["name"],
                    "format": formatted,
                    "native": assembled,
                    "hex": image.hex(),
                }
            )
    assert len(rows) == 267
    args.output.write_text(
        json.dumps(
            {
                "binary": str(binary),
                "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "reference": facts["reference"],
                "results": rows,
            },
            indent=2,
        )
        + "\n"
    )
    print(f"{len(rows)} native formatter round trips preserve all bytes")


if __name__ == "__main__":
    main()
