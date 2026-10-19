"""Bound the normal instruction path and many accepted unused cycles."""

import hashlib
import json
import subprocess
import tempfile
import time
from pathlib import Path

binaries = {
    "predecessor": Path("/private/tmp/asm198x-unused-cycles-predecessor/asm198x"),
    "current": Path("/private/tmp/asm198x-unused-cycles-build/debug/asm198x"),
}
config = json.loads(
    Path("crates/asm198x/tests/fixtures/ca65-unused-cycles.json").read_text()
)["cases"][0]["config"]
rows = []
with tempfile.TemporaryDirectory(
    prefix="asm198x-cycle-controls-performance-"
) as temporary:
    root = Path(temporary)
    (root / "input.cfg").write_text(config)
    for count in (500, 2000, 8000):
        for kind in ("instructions", "known_width_cycles"):
            source = (
                "lda #7\n" * count
                if kind == "instructions"
                else "RED=9\n"
                + "".join(
                    f".scope scope_{i}\nRED=RED+1\n.endscope\n" for i in range(count)
                )
                + ".byte 7\n"
            )
            expected = b"\xa9\x07" * count if kind == "instructions" else b"\x07"
            (root / "input.s").write_text(source)
            for name, binary in binaries.items():
                if kind == "known_width_cycles" and name == "predecessor":
                    continue
                for trial in range(3):
                    start = time.monotonic()
                    result = subprocess.run(
                        [
                            str(binary),
                            "--dialect",
                            "ca65",
                            "-C",
                            "input.cfg",
                            "input.s",
                            "-o",
                            "out.bin",
                        ],
                        cwd=root,
                        capture_output=True,
                        timeout=20,
                        check=False,
                    )
                    elapsed = time.monotonic() - start
                    assert result.returncode == 0, result.stderr
                    data = (root / "out.bin").read_bytes()
                    assert data == expected
                    rows.append(
                        {
                            "kind": kind,
                            "count": count,
                            "binary": name,
                            "trial": trial,
                            "seconds": elapsed,
                            "exit": result.returncode,
                            "output_sha256": hashlib.sha256(data).hexdigest(),
                            "source_sha256": hashlib.sha256(
                                source.encode()
                            ).hexdigest(),
                        }
                    )
Path("docs/plans/2026-10-19-ca65-unused-cycles/performance-controls.json").write_text(
    json.dumps(
        {
            "binaries": {
                k: {
                    "path": str(v),
                    "sha256": hashlib.sha256(v.read_bytes()).hexdigest(),
                }
                for k, v in binaries.items()
            },
            "results": rows,
        },
        indent=2,
    )
    + "\n"
)
print("27 bounded control measurements verified")
