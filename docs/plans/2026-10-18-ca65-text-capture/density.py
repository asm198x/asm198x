"""Compare paired macro-density runs while checking every emitted byte."""

import argparse
import hashlib
import json
import statistics
import subprocess
import tempfile
import time
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--current", type=Path, required=True)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    binaries = {"previous": args.baseline.resolve(), "current": args.current.resolve()}
    identities = {
        name: {
            "path": str(binary),
            "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        }
        for name, binary in binaries.items()
    }
    rows = []
    expected = b"\xea" * 10000
    with tempfile.TemporaryDirectory(prefix="asm198x-macro-density-") as temporary:
        root = Path(temporary)
        (root / "input.cfg").write_text(
            "MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\n"
            "SEGMENTS { CODE:load=MAIN,type=ro; }\n"
        )
        for count in [0, 50, 500]:
            source = (
                "".join(
                    f".macro macro_{index}\n.byte 1\n.endmacro\n"
                    for index in range(count)
                )
                + "nop\n" * 10000
            )
            (root / "input.s").write_text(source)
            samples: dict[str, list[float]] = {name: [] for name in binaries}
            for _ in range(5):
                for name, binary in binaries.items():
                    (root / "out.bin").unlink(missing_ok=True)
                    start = time.perf_counter()
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
                        timeout=10,
                        check=False,
                    )
                    elapsed = time.perf_counter() - start
                    assert result.returncode == 0, result.stderr
                    assert (root / "out.bin").read_bytes() == expected
                    samples[name].append(elapsed)
            assert all(len(values) == 5 for values in samples.values())
            row = {
                "definitions": count,
                "instructions": 10000,
                "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
                "output_sha256": hashlib.sha256(expected).hexdigest(),
                "samples_seconds": samples,
                "medians": {
                    name: statistics.median(values) for name, values in samples.items()
                },
            }
            rows.append(row)
            print(json.dumps(row), flush=True)
    assert len(rows) == 3
    args.output.write_text(
        json.dumps({"executables": identities, "rows": rows}, indent=2) + "\n"
    )


if __name__ == "__main__":
    main()
