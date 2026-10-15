"""Compare source-directed output placement with native SjASMPlus."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import re
import subprocess
import tempfile
from pathlib import Path


def run(argv: list[str], root: Path) -> dict:
    result = subprocess.run(
        argv, cwd=root, capture_output=True, timeout=30, check=False
    )
    return {
        "argv": [str(x).replace(str(root), "<root>") for x in argv],
        "exit": result.returncode,
        "stdout": result.stdout.decode(errors="replace").replace(str(root), "<root>"),
        "stderr": result.stderr.decode(errors="replace").replace(str(root), "<root>"),
    }


def observe(binary: Path, reference: bool, case: str, structured: bool) -> dict:
    with tempfile.TemporaryDirectory(prefix="asm198x-save-audit-") as temporary:
        container = Path(temporary).resolve()
        root = container / "work"
        root.mkdir()
        (root / "out").mkdir()
        (root / "prefix").mkdir()
        name = "saved.bin"
        flags: list[str] = []
        if case == "parent":
            name = "../saved.bin"
        elif case == "absolute":
            name = str(root / "absolute.bin")
        elif case == "relative_prefix":
            flags = ["--outprefix=prefix/"]
        elif case == "absolute_prefix":
            flags = [f"--outprefix={root}/prefix/"]
        elif case == "prefix_no_slash":
            flags = ["--outprefix=prefix/result-"]
        elif case == "missing_parent":
            name = "missing/saved.bin"
        source = (
            " DEVICE ZXSPECTRUM48\n ORG $8000\n db 42,43,44\n"
            f' SAVEBIN "{name}",$8000,1\n'
        )
        if case == "two_saves":
            source += ' SAVEBIN "second.bin",$8001,2\n'
        (root / "source.asm").write_text(source)
        (root / "out/image.bin").write_bytes(b"previous")
        if reference:
            argv = [
                str(binary),
                "--nologo",
                "--raw=out/image.bin",
                *flags,
                "source.asm",
            ]
        else:
            argv = [str(binary), "--dialect", "sjasmplus", *flags]
            if structured:
                argv.append("--message-format=json")
            argv.extend(["source.asm", "-o", "out/image.bin"])
        record = run(argv, root)
        record["files"] = {
            str(path.relative_to(container)): path.read_bytes().hex()
            for path in sorted(container.rglob("*"))
            if path.is_file() and path.name != "source.asm"
        }
        if not reference:
            if case in ["missing_parent", "prefix_no_slash"]:
                assert record["exit"] != 0
                assert record["files"] == {"work/out/image.bin": b"previous".hex()}
                if structured:
                    diagnostics = json.loads(record["stdout"])
                    assert (
                        record["stderr"] == "" and diagnostics[0]["severity"] == "Error"
                    )
                else:
                    assert "cannot write" in record["stderr"]
            else:
                assert record["exit"] == 0
                assert record["files"]["work/out/image.bin"] == "2a2b2c"
                if structured:
                    result = json.loads(record["stdout"])
                    assert result["bytes"] == [42, 43, 44]
                    assert record["stderr"] == "" and not result["diagnostics"]
                    notes = [
                        n["message"] for n in result["warnings"] if n["kind"] == "Note"
                    ]
                else:
                    notes = [
                        line
                        for line in record["stderr"].splitlines()
                        if line.startswith("wrote ")
                    ]
                saved = {
                    k: v
                    for k, v in record["files"].items()
                    if k != "work/out/image.bin"
                }
                assert len(saved) == (2 if case == "two_saves" else 1)
                for note in notes:
                    if " -> " not in note:
                        continue
                    destination = (
                        note.split(" -> ", 1)[1].strip().replace("<root>", str(root))
                    )
                    resolved = (root / destination).resolve()
                    assert resolved.is_relative_to(container)
                    assert resolved.read_bytes().hex() in saved.values()
                assert sum(" -> " in note for note in notes) == len(saved)
            record["status"] = "pass"
        return record


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("reference", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    binaries = [args.binary.resolve(), args.reference.resolve()]
    records = []
    for repeat in range(2):
        for case in [
            "relative",
            "parent",
            "absolute",
            "relative_prefix",
            "absolute_prefix",
            "prefix_no_slash",
            "missing_parent",
            "two_saves",
        ]:
            reference = observe(binaries[1], True, case, False)
            for structured in [False, True]:
                records.append(
                    {
                        "repeat": repeat,
                        "case": case,
                        "json": structured,
                        "reference": reference,
                        "candidate": observe(binaries[0], False, case, structured),
                    }
                )
    report = {
        "host": platform.platform(),
        "source_commit": "8f3813f861debf658f5329f1f8f922cb6e5b82d3",
        "identity": [
            {
                "path": str(binary),
                "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "version": run([str(binary), "--version"], Path.cwd()),
            }
            for binary in binaries
        ],
        "observations": records,
    }
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    assert len(records) == 32

    def stable(record: dict) -> dict:
        value = json.loads(json.dumps(record))
        value.pop("repeat")
        value["reference"]["stderr"] = re.sub(
            r"work time: [0-9.]+ seconds",
            "work time: <elapsed> seconds",
            value["reference"]["stderr"],
        )
        return value

    assert [stable(x) for x in records[:16]] == [stable(x) for x in records[16:]], (
        "observations changed across repetitions"
    )
    for record in records:
        c = record["candidate"]
        if record["case"] in ["relative", "two_saves"]:
            assert c["exit"] == 0 and c["files"]["work/out/image.bin"] == "2a2b2c"
    for record in records[:16]:
        print(
            json.dumps(
                {
                    "case": record["case"],
                    "json": record["json"],
                    "reference_exit": record["reference"]["exit"],
                    "candidate_exit": record["candidate"]["exit"],
                    "reference_files": record["reference"]["files"],
                    "candidate_files": record["candidate"]["files"],
                }
            )
        )


if __name__ == "__main__":
    main()
