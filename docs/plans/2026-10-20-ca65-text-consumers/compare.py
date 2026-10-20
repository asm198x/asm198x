"""Compare actual native text payloads and uniquely selected file contents."""

import argparse
import hashlib
import json
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
    facts = json.loads((ROOT / "native.json").read_text())
    rows = []
    with tempfile.TemporaryDirectory(prefix="asm198x-text-payload-compare-") as td:
        root = Path(td)
        for name, text in facts["files"].items():
            (root / name).write_text(text)
        assert len(set(facts["files"].values())) == len(facts["files"])
        for fact in facts["cases"]:
            (root / "input.s").write_text(fact["source"])
            (root / "input.cfg").write_text(fact["config"])
            outputs = ["out.bin", "out.debug198x", "out.sym", "out.lst.json"]
            for output in outputs:
                (root / output).write_bytes(b"previous output")
            native = fact["native"][0]
            value = fact["value"]
            consumer = fact["consumer"]
            headers = {
                "warning": "Warning: User warning: ",
                "error": "Error: User error: ",
                "fatal": "Fatal error: User error: ",
                "assert_error": "Error: ",
                "assert_warning": "Warning: ",
            }
            if consumer in headers:
                assert headers[consumer] + value + "\n" in native["stderr"], fact[
                    "name"
                ]
                expected = [value]
            elif consumer == "out":
                assert native["stdout"] == value + "\n", fact["name"]
                expected = [value]
            else:
                expected = []
            argv = [
                str(binary),
                "--dialect",
                "ca65",
                "-C",
                "input.cfg",
                "input.s",
                "-o",
                "out.bin",
                "--debug=out.debug198x",
                "--sym=out.sym",
                "--listing-json=out.lst.json",
                "--message-format=json",
            ]
            run = subprocess.run(
                argv, cwd=root, capture_output=True, timeout=5, check=False
            )
            stdout = run.stdout.decode(errors="replace")
            parse_error = None
            try:
                data = json.loads(stdout)
                actual = [
                    r["message"]
                    for r in (data["warnings"] if isinstance(data, dict) else data)
                ]
            except (ValueError, KeyError, TypeError) as error:
                actual = []
                parse_error = str(error)
            accepted = fact["native"][-1]["exit"] == 0
            image = (root / "out.bin").read_bytes()
            preserved = all(
                (root / output).read_bytes() == b"previous output" for output in outputs
            )
            matches = (
                run.returncode == (0 if accepted else 1)
                and not parse_error
                and actual == expected
            )
            matches = matches and (
                image.hex() == fact["native_hex"] if accepted else preserved
            )
            rows.append(
                {
                    "name": fact["name"],
                    "argv": argv,
                    "exit": run.returncode,
                    "stdout": stdout,
                    "stderr": run.stderr.decode(errors="replace"),
                    "stdout_hex": run.stdout.hex(),
                    "stderr_hex": run.stderr.hex(),
                    "hex": image.hex(),
                    "preserved": preserved,
                    "expected_messages": expected,
                    "actual_messages": actual,
                    "parse_error": parse_error,
                    "matches_native": matches,
                }
            )
    assert len(rows) == len(facts["cases"]) == 396
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
    print("cases", len(rows), "differences", sum(not r["matches_native"] for r in rows))


if __name__ == "__main__":
    main()
