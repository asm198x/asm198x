"""Capture the separate ca65 address-size override hang with bounded processes."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("assembler", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[3]
    facts = json.loads(
        (repo / "crates/asm198x/tests/fixtures/ca65-4510-phd.json").read_text()
    )
    cases = {"base_valid": " lda a:$1234\n"}
    for mnemonic in ["phd", "phw"]:
        for value in ["$1234", "$10000"]:
            cases[f"{mnemonic}_{value}"] = f".p4510\n {mnemonic} a:{value}\n"
    with tempfile.TemporaryDirectory(prefix="asm198x-4510-phd-override-") as tmp:
        root = Path(tmp)
        (root / "input.cfg").write_text(facts["cases"][0]["config"])

        def run(argv: list[str]) -> dict:
            p = subprocess.Popen(
                argv, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.PIPE
            )
            timed_out = False
            try:
                stdout, stderr = p.communicate(timeout=2)
            except subprocess.TimeoutExpired:
                timed_out = True
                p.kill()
                stdout, stderr = p.communicate(timeout=5)
            return {
                "argv": argv,
                "cwd": str(root),
                "exit": p.returncode,
                "timeout_seconds": 2,
                "timed_out": timed_out,
                "stdout": stdout.decode(errors="replace"),
                "stderr": stderr.decode(errors="replace"),
            }

        records = []
        for name, source in cases.items():
            (root / "input.s").write_text(source)
            for file in ["input.o", "native.bin", "asm.bin"]:
                (root / file).unlink(missing_ok=True)
            native = [run(["/opt/homebrew/bin/ca65", "input.s", "-o", "input.o"])]
            if native[-1]["exit"] == 0:
                native.append(
                    run(
                        [
                            "/opt/homebrew/bin/ld65",
                            "-C",
                            "input.cfg",
                            "input.o",
                            "-o",
                            "native.bin",
                        ]
                    )
                )
            assembler = run(
                [
                    str(args.assembler),
                    "--dialect",
                    "ca65",
                    "-C",
                    "input.cfg",
                    "input.s",
                    "-o",
                    "asm.bin",
                ]
            )
            assert assembler["timed_out"] and assembler["exit"] == -9
            assert all(not p["timed_out"] for p in native)
            records.append(
                {
                    "name": name,
                    "source": source,
                    "native": native,
                    "assembler": assembler,
                    "native_hex": (root / "native.bin").read_bytes().hex()
                    if (root / "native.bin").exists()
                    else None,
                }
            )

    def git(*argv: str) -> str:
        return subprocess.run(
            ["git", "-c", "core.fsmonitor=false", *argv],
            cwd=repo,
            capture_output=True,
            text=True,
            timeout=10,
            check=True,
        ).stdout.strip()

    record = {
        "source_commit": git("rev-parse", "HEAD"),
        "source_dirty": bool(git("status", "--porcelain=v1")),
        "assembler_sha256": hashlib.sha256(args.assembler.read_bytes()).hexdigest(),
        "native_tools": facts["tools"],
        "cases": records,
    }
    args.output.write_text(json.dumps(record, indent=2) + "\n")
    print("Captured five bounded hangs; all children killed and reaped")


if __name__ == "__main__":
    main()
