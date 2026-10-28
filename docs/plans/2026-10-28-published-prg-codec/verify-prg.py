"""Compare complete PRG files against ACME using original source fixtures."""

import argparse
import hashlib
import json
import subprocess
from pathlib import Path

CASES = {
    "basic_origin": "*=$0801\nlda #1\nsta $d020\nrts\n",
    "mixed_data": "*=$c000\n!byte 0,255\n!word $1234\n",
    "last_address": "*=$ffff\n!byte $42\n",
    "zero_origin": "*=$0000\nrts\n",
}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    args.output.mkdir(parents=True)
    receipts = []
    for name, source in CASES.items():
        src = args.output / f"{name}.a"
        actual = args.output / f"{name}.prg"
        reference = args.output / f"{name}-acme.prg"
        src.write_text(source)
        for command in [
            ["acme", "-f", "cbm", "-o", str(reference), str(src)],
            [str(binary), "--dialect", "acme", "--prg", "-o", str(actual), str(src)],
        ]:
            subprocess.run(command, check=True, capture_output=True, timeout=30)
        encoded = actual.read_bytes()
        assert encoded == reference.read_bytes(), name
        receipts.append(
            {
                "case": name,
                "bytes": len(encoded),
                "sha256": hashlib.sha256(encoded).hexdigest(),
            }
        )
    assert len(receipts) == 4
    (args.output / "receipt.json").write_text(json.dumps(receipts, indent=2) + "\n")
    print(json.dumps(receipts, indent=2))


if __name__ == "__main__":
    main()
