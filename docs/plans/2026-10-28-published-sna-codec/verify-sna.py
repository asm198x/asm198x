"""Compare complete SNA files against Pasmo using original source fixtures."""

import argparse
import hashlib
import json
import subprocess
from pathlib import Path

CASES = {
    "normal": "org $8000\nstart: ld a,1\nout (254),a\nret\nend start\n",
    "ram_start": "org $4000\ndefb 0,255\ndefw $1234\nend $4000\n",
    "attributes": "org $5800\ndefb $47,$01\nend $5800\n",
    "stack_overlap": "org $fffc\ndefb $aa,$bb,$cc,$dd\nend $fffc\n",
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
        src = args.output / f"{name}.asm"
        actual = args.output / f"{name}.sna"
        reference = args.output / f"{name}-pasmo.sna"
        src.write_text(source)
        for command in [
            ["pasmo", "--sna", str(src), str(reference)],
            [str(binary), "--dialect", "pasmo", "--sna", "-o", str(actual), str(src)],
        ]:
            subprocess.run(command, check=True, capture_output=True, timeout=30)
        encoded = actual.read_bytes()
        pasmo = reference.read_bytes()
        differences = [
            i for i, (a, b) in enumerate(zip(encoded, pasmo, strict=True)) if a != b
        ]
        # Existing Asm198x policy keeps assembled attribute bytes. Pasmo fills
        # attributes after placing the program, overwriting these two bytes.
        if name == "attributes":
            assert differences == [27 + 0x1800, 27 + 0x1801], differences
            assert encoded[27 + 0x1800 : 27 + 0x1802] == bytes([0x47, 0x01])
            assert pasmo[27 + 0x1800 : 27 + 0x1802] == bytes([0x38, 0x38])
        else:
            assert not differences, (name, differences)
        assert len(encoded) == 49179, name
        receipts.append(
            {
                "case": name,
                "bytes": len(encoded),
                "pasmo_differing_offsets": differences,
                "sha256": hashlib.sha256(encoded).hexdigest(),
            }
        )
    assert len(receipts) == 4
    (args.output / "receipt.json").write_text(json.dumps(receipts, indent=2) + "\n")
    print(json.dumps(receipts, indent=2))


if __name__ == "__main__":
    main()
