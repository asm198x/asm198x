import hashlib
import json
import subprocess
import tempfile
import time
from pathlib import Path

binaries = {
    "predecessor": Path(
        "/private/tmp/asm198x-scope-binding-predecessor/asm198x-aarch64-apple-darwin/asm198x"
    ),
    "current": Path("/private/tmp/asm198x-scope-binding-build/debug/asm198x"),
}
facts = json.loads(
    Path("crates/asm198x/tests/fixtures/ca65-scope-binding.json").read_text()
)
rows = []
with tempfile.TemporaryDirectory(prefix="asm198x-scope-performance-") as td:
    root = Path(td)
    (root / "input.cfg").write_text(facts["cases"][0]["config"])
    for n in (500, 2000, 8000):
        for kind in ("independent", "forward_chain"):
            source = (
                (
                    "".join(f"VALUE_{i}={i % 256}\n" for i in range(n))
                    + f".byte VALUE_{n - 1}\n"
                )
                if kind == "independent"
                else "".join(f"LINK_{i}=LINK_{i + 1}\n" for i in range(n))
                + f"LINK_{n}=$34\n.byte LINK_0\n"
            )
            expected = bytes([(n - 1) % 256]) if kind == "independent" else b"\x34"
            (root / "input.s").write_text(source)
            for name, binary in binaries.items():
                for trial in range(3):
                    start = time.monotonic()
                    p = subprocess.run(
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
                        check=False,
                        timeout=20,
                    )
                    elapsed = time.monotonic() - start
                    assert p.returncode == 0, p.stderr
                    data = (root / "out.bin").read_bytes()
                    assert data == expected
                    rows.append(
                        {
                            "kind": kind,
                            "definitions": n,
                            "binary": name,
                            "trial": trial,
                            "seconds": elapsed,
                            "exit": p.returncode,
                            "hex": data.hex(),
                        }
                    )
Path("docs/plans/2026-10-19-ca65-scope-binding/performance.json").write_text(
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
print("36 bounded measurements verified")
