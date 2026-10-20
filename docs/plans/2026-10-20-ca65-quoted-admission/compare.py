"""Replay retained lexical facts, including first-error source positions."""

import argparse
import hashlib
import json
import re
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).parent


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    facts = json.loads((ROOT / "native-expanded.json").read_text())
    rows = []
    with tempfile.TemporaryDirectory(prefix="asm198x-quoted-admission-") as temp:
        root = Path(temp)
        for case in facts["cases"]:
            (root / "input.s").write_text(case["source"])
            (root / "input.cfg").write_text(case["config"])
            outputs = ["out.bin", "out.sym", "out.debug198x", "out.lst.json"]
            for output in outputs:
                (root / output).write_bytes(b"previous output")
            argv = [
                str(binary),
                "--dialect",
                "ca65",
                "-C",
                "input.cfg",
                "input.s",
                "-o",
                "out.bin",
                "--sym=out.sym",
                "--debug=out.debug198x",
                "--listing-json=out.lst.json",
                "--message-format=json",
            ]
            run = subprocess.run(
                argv, cwd=root, capture_output=True, timeout=10, check=False
            )
            payload = json.loads(run.stdout)
            accepted = case["native"][-1]["exit"] == 0
            native_error = next(
                (
                    re.search(
                        r"input\.s\((\d+)\):.*(?:Error|Fatal error):", stage["stderr"]
                    )
                    for stage in case["native"]
                    if stage["exit"] != 0
                ),
                None,
            )
            native_line = int(native_error[1]) if native_error else None
            actual_span = (
                payload[0]["span"] if isinstance(payload, list) and payload else None
            )
            preserved = all(
                (root / output).read_bytes() == b"previous output" for output in outputs
            )
            matches = run.returncode == (0 if accepted else 1)
            matches = matches and (
                (root / "out.bin").read_bytes().hex() == case["native_hex"]
                if accepted
                else preserved
                and actual_span is not None
                and actual_span["path"] == "input.s"
                and actual_span["line"] == native_line
            )
            rows.append(
                {
                    "name": case["name"],
                    "argv": argv,
                    "exit": run.returncode,
                    "stdout": run.stdout.decode(),
                    "stderr": run.stderr.decode(),
                    "native_error_line": native_line,
                    "actual_span": actual_span,
                    "preserved": preserved,
                    "matches_native": matches,
                }
            )
    assert len(rows) == len(facts["cases"]) == 549
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
