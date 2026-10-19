"""Measure enum defaults and forward chains with explicit executables."""

import hashlib
import json
import subprocess
import tempfile
import time
from pathlib import Path

binaries = {
    "predecessor": Path("/private/tmp/asm198x-enum-execution-predecessor/asm198x"),
    "current": Path("/private/tmp/asm198x-enum-execution-build/debug/asm198x"),
}
config = json.loads(
    Path("crates/asm198x/tests/fixtures/ca65-enum-expressions.json").read_text()
)["cases"][0]["config"]
rows = []
with tempfile.TemporaryDirectory(prefix="asm198x-enum-performance-") as td:
    root = Path(td)
    (root / "input.cfg").write_text(config)
    for count in (500, 2000, 8000):
        for kind in ("defaults", "forward_chain"):
            if kind == "defaults":
                source = (
                    ".enum Colors\nVALUE_0=2\n"
                    + "".join(f"VALUE_{i}\n" for i in range(1, count))
                    + f".endenum\n.word Colors::VALUE_{count - 1}\n"
                )
                expected = (count + 1).to_bytes(2, "little")
            else:
                source = (
                    ".enum Colors\n"
                    + "".join(f"VALUE_{i}=VALUE_{i + 1}\n" for i in range(count))
                    + f"VALUE_{count}=4\n.endenum\n.word Colors::VALUE_0\n"
                )
                expected = b"\x04\x00"
            (root / "input.s").write_text(source)
            for name, binary in binaries.items():
                if kind == "forward_chain" and name == "predecessor":
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
                            "members": count,
                            "binary": name,
                            "trial": trial,
                            "seconds": elapsed,
                            "exit": result.returncode,
                            "hex": data.hex(),
                            "source_sha256": hashlib.sha256(
                                source.encode()
                            ).hexdigest(),
                        }
                    )
Path("docs/plans/2026-10-19-ca65-enum-expressions/performance.json").write_text(
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
print("27 bounded enum measurements verified")
