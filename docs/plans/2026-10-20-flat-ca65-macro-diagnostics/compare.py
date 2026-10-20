"""Compare native outcomes, locations and authored macro-definition context."""

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
    with tempfile.TemporaryDirectory(prefix="asm198x-flat-macro-context-") as temp:
        root = Path(temp)
        for name, source in facts["files"].items():
            (root / name).write_text(source)
        for case in facts["cases"]:
            (root / "input.s").write_text(case["source"])
            outputs = ["out.bin", "out.debug198x", "out.sym", "out.lst.json"]
            for output in outputs:
                (root / output).write_bytes(b"previous output")
            argv = [
                str(binary),
                "--dialect",
                case["name"].split("_", 1)[0],
                "input.s",
                "-o",
                "out.bin",
                "--debug=out.debug198x",
                "--sym=out.sym",
                "--listing-json=out.lst.json",
                "--message-format=json",
            ]
            result = subprocess.run(
                argv, cwd=root, capture_output=True, text=True, timeout=5, check=False
            )
            payload = json.loads(result.stdout)
            accepted = case["native"][-1]["exit"] == 0
            preserved = all(
                (root / output).read_bytes() == b"previous output" for output in outputs
            )
            outcome = result.returncode == (0 if accepted else 1) and (
                (root / "out.bin").read_bytes().hex() == case["native_hex"]
                if accepted
                else preserved
            )
            context = accepted
            expected = None
            if not accepted and result.returncode == 1:
                native = re.search(
                    r"^(.+?)\((\d+)\):.*(?:Error|Fatal error):",
                    case["native"][0]["stderr"],
                    re.MULTILINE,
                )
                assert native is not None
                filename, line = native[1], int(native[2])
                span = payload[0]["span"]
                actual_path = span["path"].replace("\\", "/").rsplit("/", 1)[-1]
                names = (
                    []
                    if case["name"].endswith("_plain")
                    else (
                        ["inner", "outer"] if "_nested_" in case["name"] else ["inner"]
                    )
                )
                authored = (
                    case["source"]
                    if filename == "input.s"
                    else facts["files"][filename]
                )
                definitions = {
                    words[1]: i
                    for i, text in enumerate(authored.splitlines(), 1)
                    if (words := text.split()) and words[0] == ".macro"
                }
                expected = {
                    "path": filename,
                    "line": line,
                    "frames": names,
                    "definition_lines": definitions,
                }
                frames = span["expansion_frames"]
                context = (
                    actual_path == filename
                    and span["line"] == line
                    and [f["macro_name"] for f in frames] == names
                )
                for frame in frames:
                    definition, invocation = (
                        frame.get("defined_at"),
                        frame["invoked_at"],
                    )
                    context = (
                        context
                        and definition is not None
                        and definition["path"].replace("\\", "/").rsplit("/", 1)[-1]
                        == filename
                    )
                    context = (
                        context
                        and definition["line"] == definitions[frame["macro_name"]]
                    )
                    context = (
                        context
                        and invocation["path"].replace("\\", "/").rsplit("/", 1)[-1]
                        == filename
                    )
            rows.append(
                {
                    "name": case["name"],
                    "argv": argv,
                    "exit": result.returncode,
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                    "native_accepted": accepted,
                    "matches_native_outcome": outcome,
                    "matches_context": context,
                    "expected": expected,
                }
            )
    assert len(rows) == 80
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
        "outcome differences",
        sum(not r["matches_native_outcome"] for r in rows),
        "context differences",
        sum(not r["matches_context"] for r in rows),
    )


if __name__ == "__main__":
    main()
