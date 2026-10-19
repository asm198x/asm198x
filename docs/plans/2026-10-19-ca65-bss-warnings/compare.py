"""Compare explicit binaries against native byte, status and warning facts."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("binary", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()
binary = args.binary.resolve(strict=True)
facts = json.loads(Path(__file__).with_name("selected-native.json").read_text())
rows = []
with tempfile.TemporaryDirectory(prefix="asm198x-bss-warning-compare-") as t:
    root = Path(t)
    for name, text in facts["files"].items():
        (root / name).write_text(text)
    for fact in facts["cases"]:
        (root / "input.s").write_text(fact["source"])
        (root / "input.cfg").write_text(fact["config"])
        (root / "out.bin").unlink(missing_ok=True)
        argv = [
            str(binary),
            "--dialect",
            "ca65",
            "-C",
            "input.cfg",
            "--message-format",
            "json",
            "input.s",
            "-o",
            "out.bin",
        ]
        result = subprocess.run(
            argv, cwd=root, capture_output=True, text=True, timeout=5, check=False
        )
        accepted = fact["native"][-1]["exit"] == 0
        output = json.loads(result.stdout)
        count = (
            sum("contains initialized data" in w["message"] for w in output["warnings"])
            if result.returncode == 0
            else 0
        )
        data = (
            (root / "out.bin").read_bytes().hex()
            if (root / "out.bin").exists()
            else None
        )
        rows.append(
            {
                "name": fact["name"],
                "argv": argv,
                "exit": result.returncode,
                "stdout": result.stdout,
                "stderr": result.stderr,
                "hex": data,
                "warning_count": count,
                "native_warning_count": fact["native_warning_count"],
                "matches": result.returncode == (0 if accepted else 1)
                and (
                    not accepted
                    or data == fact["native_hex"]
                    and count == fact["native_warning_count"]
                ),
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
print(len(rows), "comparisons;", sum(not r["matches"] for r in rows), "disagreements")
for row in rows:
    if not row["matches"]:
        print(
            row["name"], row["exit"], row["warning_count"], row["native_warning_count"]
        )
