"""Audit ordinary disassembly using an explicitly supplied released executable."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import re
import subprocess
import tempfile
from pathlib import Path

FLAGS = {
    "output_short": ["-o", "out.s"],
    "output_long": ["--output", "out.s"],
    "container_prg": ["--prg"],
    "container_sna": ["--sna"],
    "container_tap": ["--tap"],
    "container_tzx": ["--tzx"],
    "container_exe": ["--exe"],
    "container_gb": ["--gb-rom"],
    "linker_config": ["-C", "missing.cfg"],
    "outprefix": ["--outprefix", "missing-prefix/"],
    "equ": ["--equ", "ONE=1"],
    "include_dir": ["-I", "missing-includes/"],
    "json": ["--message-format=json"],
}
CONTROLS = [
    "linear_6502",
    "linear_z80",
    "linear_68000",
    "valid_assembly",
    "invalid_cpu",
    "invalid_sidecar",
    "recovery_new",
    "recovery_existing",
    "format_acme",
]


def invoke(binary: Path, args: list[str], root: Path) -> dict:
    process = subprocess.run(
        [str(binary), *args], cwd=root, capture_output=True, timeout=30, check=False
    )
    return {
        "argv": ["<asm198x>", *args],
        "exit": process.returncode,
        "stdout": process.stdout.decode(errors="replace").replace(str(root), "<case>"),
        "stderr": process.stderr.decode(errors="replace").replace(str(root), "<case>"),
    }


def branch_target(text: str) -> int | None:
    match = re.search(r"\bbra(?:\.\w+)?\s+\$([0-9a-f]+)", text, re.IGNORECASE)
    return int(match[1], 16) if match else None


def snapshot(root: Path) -> dict[str, str]:
    return {
        str(p.relative_to(root)): p.read_bytes().hex()
        for p in sorted(root.rglob("*"))
        if p.is_file()
    }


def observe(binary: Path, name: str) -> dict:
    with tempfile.TemporaryDirectory(prefix="asm198x-disasm-audit-") as temporary:
        root = Path(temporary).resolve()
        (root / "input.bin").write_bytes(bytes.fromhex("a90560"))
        (root / "input.asm").write_text("*=$8000\n lda #$05\n rts\n")
        (root / "out.s").write_text("preserve previous source\n")
        args = ["disasm", "--dialect", "acme", "--org", "0x8000", "input.bin"]
        if name in FLAGS:
            args[1:1] = FLAGS[name]
        elif name == "linear_z80":
            (root / "input.bin").write_bytes(bytes.fromhex("1802"))
            args = ["disasm", "--dialect", "pasmo", "--org", "0x8000", "input.bin"]
        elif name in ["linear_68000", "origin_68000_high"]:
            (root / "input.bin").write_bytes(bytes.fromhex("6002"))
            args = [
                "disasm",
                "--dialect",
                "vasm",
                "--org",
                "0x8000" if name == "linear_68000" else "0x10000",
                "input.bin",
            ]
        elif name == "valid_assembly":
            args = ["--dialect", "acme", "input.asm", "-o", "image.bin"]
        elif name == "invalid_cpu":
            args[1:1] = ["--cpu", "nonsense"]
        elif name == "invalid_sidecar":
            args[1:1] = ["--debug=out.s"]
        elif name in ["recovery_new", "recovery_existing"]:
            args = [
                "disasm",
                "--recover",
                "--org",
                "0x8000",
                "--code",
                "0x8000:0x8003",
                "input.bin",
                "-o",
                "recovered.a" if name == "recovery_new" else "out.s",
            ]
        elif name == "format_acme":
            args = ["fmt", "--dialect", "acme", "input.asm", "-o", "formatted.a"]
        before = snapshot(root)
        result = invoke(binary, args, root)
        extra = []
        if name in ["recovery_new", "format_acme"] and result["exit"] == 0:
            source = "recovered.a" if name == "recovery_new" else "formatted.a"
            extra.append(invoke(binary, [source, "-o", "rebuilt.bin"], root))
        after = snapshot(root)
        if name in FLAGS:
            passed = result["exit"] > 0 and before == after
            classification = "pass" if passed else "violation"
        elif name == "origin_68000_high":
            classification = (
                "pass"
                if result["exit"] == 0 and branch_target(result["stdout"]) == 0x10004
                else "limitation"
            )
        else:
            if name in ["invalid_cpu", "invalid_sidecar", "recovery_existing"]:
                assert result["exit"] > 0 and before == after, (
                    f"positive rejection control failed: {name}"
                )
            else:
                assert result["exit"] == 0, (
                    f"positive success control failed: {name}: {result}"
                )
                if name == "linear_6502":
                    assert "LDA #$05" in result["stdout"] and "RTS" in result["stdout"]
                elif name == "linear_z80":
                    assert (
                        "JR" in result["stdout"].upper() and "8004" in result["stdout"]
                    )
                elif name == "linear_68000":
                    assert branch_target(result["stdout"]) == 0x8004
                elif name == "valid_assembly":
                    assert after["image.bin"] == "a90560"
                else:
                    assert (
                        extra
                        and extra[0]["exit"] == 0
                        and after["rebuilt.bin"] == "a90560"
                    )
            classification = "pass"
        assert all(after.get(name) == value for name, value in before.items()), (
            "input/seeded source changed"
        )
        return {
            "name": name,
            "classification": classification,
            "process": result,
            "additional_processes": extra,
            "before": before,
            "after": after,
        }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve()
    names = [*CONTROLS, *FLAGS, "origin_68000_high"]
    assert len(names) == 23 and len(set(names)) == 23
    observations = [
        {"repeat": repeat, **observe(binary, name)}
        for repeat in range(2)
        for name in names
    ]
    assert [
        {k: v for k, v in x.items() if k != "repeat"} for x in observations[:23]
    ] == [{k: v for k, v in x.items() if k != "repeat"} for x in observations[23:]], (
        "repeated observations changed"
    )
    help_result = invoke(binary, ["--help"], Path.cwd())
    report = {
        "version": "0.0.66",
        "source_commit": "35a07893a57cbb42c3a341cc293fc4b713af99f4",
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "host": platform.platform(),
        "python": platform.python_version(),
        "help": help_result,
        "observations": observations,
    }
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(
        json.dumps(
            {
                "observations": len(observations),
                "counts": {
                    state: sum(x["classification"] == state for x in observations)
                    for state in ["pass", "violation", "limitation"]
                },
            }
        )
    )


if __name__ == "__main__":
    main()
