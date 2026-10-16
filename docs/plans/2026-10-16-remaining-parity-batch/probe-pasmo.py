"""Capture bounded native Pasmo memory-wrap facts; never execute upstream code."""

import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

CASES = {
    "decimal_equ": "V equ 65536\n ld hl,V\n",
    "prefixed_hex_equ": "V equ $10000\n ld hl,V\n",
    "end_label": " org $ffff\n db $a5\nafter:\n",
    "prefixed_hex_overflow": " org $10000\n db 7\n",
    "prefixed_binary_overflow": " dw %10000000000000000\n",
    "one_at_end": " org $ffff\n db $a5\n",
    "two_cross": " org $ffff\n db $a5,$5a\n",
    "subsequent_pc": " org $ffff\nfirst: db $a5,$5a\nafter: dw after,$\n end after\n",
    "negative_count": " org $8000\n defs -1,$a5\nafter:\n",
    "count_zero": " org $ffff\n defs 65536,$a5\nafter: db $5a\n",
    "count_one": " org $ffff\n defs -65535,$a5\nafter: db $5a\n",
    "full_image": " org 0\n defs 65535,$a5\n db $5a\nafter:\n",
    "second_write": " org $ffff\n db $a5,$5a\n org 0\n db $bb\n",
    "backward_org": " org $9000\n db $a5\n org $8000\n db $5a\n",
    "forward_gap": " org $8000\n db $a5\n org $8003\n db $5a\n",
    "overwrite_after_gap": " org $8000\n db $a5,$bb\n org $7fff\n db $5a\n org $8001\n db $cc\n",
    "multi_wrap": " org $ffff\n defs 65535,$a5\n defs 3,$5a\n",
    "branch_forward": " org $ffff\n jr target\ntarget: db $a5\n",
    "branch_backward": " org $ffff\ntarget: db $a5\n jr target\n",
    "high_org": " org $1ffff\n db $a5,$5a\n",
    "fill_pc": " org $ffff\n defs 2,$\n",
    "negative_zero": " org 0\n defs -1,0\n",
}


def main() -> None:
    tool = Path(shutil.which("pasmo") or "/usr/local/bin/pasmo").resolve()
    version = subprocess.run(
        [str(tool)], capture_output=True, text=True, timeout=10, check=False
    )
    records = []
    with tempfile.TemporaryDirectory(prefix="asm198x-wrap-probe-") as directory:
        root = Path(directory)
        for name, source in CASES.items():
            for path in [root / "output.bin", root / "output.sym"]:
                path.unlink(missing_ok=True)
            (root / "input.asm").write_text(source)
            argv = [str(tool), "--bin", "input.asm", "output.bin", "output.sym"]
            result = subprocess.run(
                argv, cwd=root, capture_output=True, text=True, timeout=10, check=False
            )
            image = root / "output.bin"
            record = {
                "name": name,
                "source": source,
                "argv": argv,
                "exit": result.returncode,
                "stdout": result.stdout,
                "stderr": result.stderr,
                "hex": image.read_bytes().hex() if image.exists() else None,
                "symbols": (root / "output.sym").read_text()
                if (root / "output.sym").exists()
                else None,
            }
            records.append(record)
            print(
                name,
                result.returncode,
                image.stat().st_size if image.exists() else None,
                record["symbols"],
                result.stderr.strip(),
            )
    destination = Path("crates/asm198x/tests/fixtures/pasmo-output-wrap.json")
    destination.write_text(
        json.dumps(
            {
                "tool": str(tool),
                "sha256": hashlib.sha256(tool.read_bytes()).hexdigest(),
                "version": version.stderr,
                "cases": records,
            },
            indent=2,
        )
        + "\n"
    )


if __name__ == "__main__":
    main()
