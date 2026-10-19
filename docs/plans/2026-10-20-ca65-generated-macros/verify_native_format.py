"""Verify complete canonical source against native ca65 and ld65."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--predecessor", required=True, type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    predecessor = args.predecessor.resolve(strict=True)
    facts = json.loads(
        (
            Path(__file__).parents[3]
            / "crates/asm198x/tests/fixtures/ca65-generated-macros.json"
        ).read_text()
    )
    for tool in facts["tools"].values():
        assert (
            hashlib.sha256(Path(tool["path"]).read_bytes()).hexdigest()
            == tool["sha256"]
        )
    rows = []
    with tempfile.TemporaryDirectory(prefix="asm198x-canonical-native-format-") as td:
        root = Path(td)
        for name, text in facts["files"].items():
            (root / name).write_text(text)

        def run(argv: list[str]) -> dict:
            p = subprocess.run(
                argv, cwd=root, capture_output=True, text=True, timeout=5, check=False
            )
            return {
                "argv": argv,
                "exit": p.returncode,
                "stdout": p.stdout,
                "stderr": p.stderr,
            }

        for fact in facts["cases"]:
            (root / "input.s").write_text(fact["source"])
            (root / "input.cfg").write_text(fact["config"])
            fmt = run([str(binary), "fmt", "--dialect", "ca65", "input.s"])
            assert fmt["exit"] == 0, (fact["name"], fmt)
            (root / "formatted.s").write_text(fmt["stdout"])
            again = run([str(binary), "fmt", "--dialect", "ca65", "formatted.s"])
            assert again["exit"] == 0 and again["stdout"] == fmt["stdout"], fact["name"]
            for name in ["formatted.o", "native.bin"]:
                (root / name).unlink(missing_ok=True)
            native = [
                run(
                    [facts["tools"]["ca65"]["path"], "formatted.s", "-o", "formatted.o"]
                )
            ]
            if native[0]["exit"] == 0:
                native.append(
                    run(
                        [
                            facts["tools"]["ld65"]["path"],
                            "-C",
                            "input.cfg",
                            "formatted.o",
                            "-o",
                            "native.bin",
                        ]
                    )
                )
            accepted = fact["native"][-1]["exit"] == 0
            assert (native[-1]["exit"] == 0) == accepted, (fact["name"], native)
            image = (root / "native.bin").read_bytes().hex() if accepted else None
            if accepted:
                assert image == fact["native_hex"], fact["name"]
            assembled = {}
            for name in ["input.s", "formatted.s"]:
                (root / "out.bin").write_bytes(b"previous image")
                result = run(
                    [
                        str(binary),
                        "--dialect",
                        "ca65",
                        "-C",
                        "input.cfg",
                        name,
                        "-o",
                        "out.bin",
                    ]
                )
                result["hex"] = (root / "out.bin").read_bytes().hex()
                if result["exit"] == 0:
                    if accepted:
                        assert result["hex"] == fact["native_hex"], fact["name"]
                    else:
                        # #853 already accepts some invalid register labels on
                        # their own line. Prove each formatted false acceptance
                        # predates this slice, rather than grandfathering a new one.
                        assert name == "formatted.s", fact["name"]
                        before_argv = [str(predecessor), *result["argv"][1:]]
                        (root / "out.bin").write_bytes(b"previous image")
                        before = run(before_argv)
                        before["hex"] = (root / "out.bin").read_bytes().hex()
                        assert before["exit"] == 0 and before["hex"] == result["hex"], (
                            fact["name"],
                            before,
                        )
                        result["existing_formatted_gap"] = 853
                        result["predecessor"] = before
                else:
                    if accepted:
                        assert fact.get("assembly_gaps"), fact["name"]
                    assert result["hex"] == b"previous image".hex(), fact["name"]
                assembled[name] = result
            rows.append(
                {
                    "name": fact["name"],
                    "format": fmt,
                    "again": again,
                    "native": native,
                    "native_hex": image,
                    "assembled": assembled,
                }
            )
    assert len(rows) == 96
    args.output.write_text(
        json.dumps(
            {
                "binary": str(binary),
                "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "cases": rows,
                "predecessor": {
                    "path": str(predecessor),
                    "sha256": hashlib.sha256(predecessor.read_bytes()).hexdigest(),
                },
            },
            indent=2,
        )
        + "\n"
    )
    print("96 native formatter outcomes retained; all 67 accepted images match")
    print(
        "pre-existing formatted register false acceptances:",
        [
            r["name"]
            for r in rows
            if r["assembled"]["formatted.s"].get("existing_formatted_gap")
        ],
    )
    print(
        "remaining original assembly gaps:",
        [
            r["name"]
            for r in rows
            if r["native"][-1]["exit"] == 0 and r["assembled"]["input.s"]["exit"] != 0
        ],
    )
    print(
        "remaining formatted assembly gaps:",
        [
            r["name"]
            for r in rows
            if r["native"][-1]["exit"] == 0
            and r["assembled"]["formatted.s"]["exit"] != 0
        ],
    )


if __name__ == "__main__":
    main()
