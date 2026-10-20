"""Compare native text payloads as well as acceptance and emitted images."""

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
    facts = json.loads((ROOT / "text-consumer-native.json").read_text())
    rows = []
    with tempfile.TemporaryDirectory(prefix="asm198x-text-consumer-") as temporary:
        root = Path(temporary)
        for name, source in facts["files"].items():
            (root / name).write_text(source)
        for case in facts["cases"]:
            (root / "input.s").write_text(case["source"])
            (root / "input.cfg").write_text(case["config"])
            (root / "out.bin").write_bytes(b"previous image")
            argv = [
                str(binary),
                "--dialect",
                "ca65",
                "-C",
                "input.cfg",
                "input.s",
                "-o",
                "out.bin",
                "--message-format=json",
            ]
            run = subprocess.run(
                argv, cwd=root, capture_output=True, text=True, timeout=5, check=False
            )
            output = json.loads(run.stdout)
            actual_messages = (
                [warning["message"] for warning in output["warnings"]]
                if isinstance(output, dict)
                else [diagnostic["message"] for diagnostic in output]
            )
            native_messages = re.findall(
                r"(?:Warning: User warning: |Error: User error: |Error: )(.*)\n",
                case["native"][0]["stderr"],
            )
            accepted = case["native"][-1]["exit"] == 0
            image = (root / "out.bin").read_bytes()
            matches = (
                run.returncode == (0 if accepted else 1)
                and (
                    image.hex() == case["native_hex"]
                    if accepted
                    else image == b"previous image"
                )
                and actual_messages == native_messages
            )
            rows.append(
                {
                    "name": case["name"],
                    "argv": argv,
                    "exit": run.returncode,
                    "stdout": run.stdout,
                    "stderr": run.stderr,
                    "hex": image.hex(),
                    "native_messages": native_messages,
                    "actual_messages": actual_messages,
                    "matches_native": matches,
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
    print(
        "cases",
        len(rows),
        "differences",
        sum(not row["matches_native"] for row in rows),
    )


if __name__ == "__main__":
    main()
