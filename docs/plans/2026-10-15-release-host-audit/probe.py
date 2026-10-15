#!/usr/bin/env python3
"""Record release findings; process success means a completed, controlled audit."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.request
import zipfile
from pathlib import Path
from typing import Any

VERSION = "0.0.65"
SOURCE = "fa06f5617f32bec52ac14f3a25bdd8e13dae9287"
UPSTREAM = "35b4968be3bdd26804a5d195557d1225497a0358"
FILES = {
    "LICENSE": "62cd567bc9270c722abcaa6ad88b6a1fe861698d63890f787cc557d2395be203",
    "helloworld.asm": "9e57c82435c0cf35e56089d56d6b83991c61624c2b3448ab6c66fb13a884a0a5",
    "libs/loader.asm": "32b0dfd80eacbbdfa69774280122d9c7f115fe0602a14bc5d630b24e08cd82cc",
    "libs/screen.asm": "d33fbb4f0d549567438138efa38adbb4eb154aa17d6e1c0dee496f065cf469c6",
}
ARCHIVES = {
    "aarch64-apple-darwin": "7b4dcef1b0b6fa164eabf47293e10bbada3d8e5899a6766d2e3cfe35f141db8c",
    "x86_64-apple-darwin": "7b3d122ad26dcda50a13c8b2f3ee48f3a2f8afeb00d18ec9e0b952f858c0893d",
    "aarch64-unknown-linux-gnu": "e79305ee5208670ddbc40a97c4d3725575ce7efe35d5fcf71d961efb3fd3fc93",
    "x86_64-unknown-linux-gnu": "02cd6c868f100a37d46fe6283927b13583594e620f966c0865f3f0a705f2d20e",
    "x86_64-pc-windows-msvc": "5a94fa17553411f337b37c4278873140c9bad2c66fe3e78e5d986382be877f93",
}


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def verified(data: bytes, expected: str) -> bytes:
    if sha(data) != expected:
        raise ValueError("checksum mismatch")
    return data


def fetch(url: str, path: Path) -> bytes:
    if not path.exists():
        path.parent.mkdir(parents=True, exist_ok=True)
        for attempt in range(3):
            try:
                with urllib.request.urlopen(url, timeout=45) as response:
                    data = response.read(10_000_001)
                if len(data) > 10_000_000:
                    raise ValueError("audit download exceeded 10 MB")
                path.write_bytes(data)
                break
            except OSError:
                if attempt == 2:
                    raise
    return path.read_bytes()


def audit(
    target: str, cache: Path, supplied_binary: Path | None, corrupt_output: bool = False
) -> dict[str, Any]:
    archive = f"asm198x-{target}." + ("zip" if "windows" in target else "tar.xz")
    base = f"https://github.com/asm198x/asm198x/releases/download/asm198x-v{VERSION}/"
    data = verified(fetch(base + archive, cache / archive), ARCHIVES[target])
    sidecar = (
        fetch(base + archive + ".sha256", cache / (archive + ".sha256"))
        .decode()
        .split()
    )
    assert sidecar == [sha(data), archive] or sidecar == [sha(data), "*" + archive], (
        sidecar
    )
    if "windows" in target:
        with zipfile.ZipFile(cache / archive) as package:
            executable = package.read("asm198x.exe")
    else:
        with tarfile.open(cache / archive) as package:
            member = package.extractfile(f"asm198x-{target}/asm198x")
            assert member is not None
            executable = member.read()
    # Corrupt checksum and wrong-byte controls must both go red.
    try:
        verified(data + b"corrupt", ARCHIVES[target])
    except ValueError:
        checksum_control = True
    else:
        checksum_control = False
    assert checksum_control
    results: list[dict[str, Any]] = []
    with tempfile.TemporaryDirectory(prefix="asm198x-host-audit-") as directory:
        root = Path(directory)
        binary = root / ("asm198x.exe" if "windows" in target else "asm198x")
        binary.write_bytes(executable)
        binary.chmod(0o755)
        if supplied_binary is not None:
            assert sha(supplied_binary.read_bytes()) == sha(executable), (
                "installed/archive binary mismatch"
            )
            binary = supplied_binary.resolve()

        def record(
            name: str,
            checks: dict[str, bool],
            runs: list[dict[str, Any]],
            **details: Any,
        ) -> None:
            assert checks, name
            results.append(
                dict(
                    name=name,
                    status="pass" if all(checks.values()) else "fail",
                    checks=checks,
                    runs=runs,
                    **details,
                )
            )

        def run(args: list[str], cwd: Path = root) -> dict[str, Any]:
            start = time.monotonic()
            argv = [str(binary), *args]
            try:
                p = subprocess.run(
                    argv, cwd=cwd, capture_output=True, timeout=30, check=False
                )
                return {
                    "argv": argv,
                    "cwd": str(cwd),
                    "exit": p.returncode,
                    "stdout": p.stdout.decode("utf-8", errors="replace"),
                    "stderr": p.stderr.decode("utf-8", errors="replace"),
                    "seconds": round(time.monotonic() - start, 4),
                }
            except subprocess.TimeoutExpired:
                return {
                    "argv": argv,
                    "cwd": str(cwd),
                    "exit": None,
                    "stdout": "",
                    "stderr": "host timeout",
                    "seconds": 30,
                }

        def write(name: str, text: str) -> None:
            (root / name).write_bytes(text.encode())

        def bytes_at(name: str) -> bytes | None:
            p = root / name
            return p.read_bytes() if p.exists() else None

        def parsed(result: dict[str, Any]) -> Any:
            try:
                return json.loads(result["stdout"])
            except ValueError:
                return None

        # The published installer is a separate installation path. Unmanaged
        # mode confines writes to this disposable directory and disables profiles/receipts.
        windows = "windows" in target
        script_name = "asm198x-installer." + ("ps1" if windows else "sh")
        script_hash = (
            "0826e238ecbefb40aa7d31050c73eb66d917a357d919dd8900d75a4b8f1414e1"
            if windows
            else "477e3cb7d775d9602b0177bcf8b760877b397826f36883122af88f0dc6d293da"
        )
        script = cache / script_name
        verified(fetch(base + script_name, script), script_hash)
        shell = shutil.which("pwsh") if windows else shutil.which("sh")
        if not shell or (
            not windows and not (shutil.which("curl") or shutil.which("wget"))
        ):
            results.append(
                {
                    "name": "published_installer",
                    "status": "unavailable",
                    "reason": "installer needs PowerShell or a POSIX shell with curl/wget",
                }
            )
        else:
            installed = root / "installed"
            env = os.environ.copy()
            for key in ["ASM198X_INSTALL_DIR", "CARGO_DIST_FORCE_INSTALL_DIR"]:
                env.pop(key, None)
            env["ASM198X_UNMANAGED_INSTALL"] = str(installed)
            command = (
                [shell, "-NoProfile", "-File", str(script)]
                if windows
                else [shell, str(script)]
            )
            p = subprocess.run(
                command, capture_output=True, timeout=120, check=False, env=env
            )
            installed_binary = installed / ("asm198x.exe" if windows else "asm198x")
            record(
                "published_installer",
                {
                    "success": p.returncode == 0,
                    "archive_binary": installed_binary.is_file()
                    and sha(installed_binary.read_bytes()) == sha(executable),
                },
                [
                    {
                        "argv": command,
                        "exit": p.returncode,
                        "stdout": p.stdout.decode(errors="replace"),
                        "stderr": p.stderr.decode(errors="replace"),
                        "unmanaged_install": str(installed),
                    }
                ],
                installer_sha256=script_hash,
            )

        p = run(["--version"])
        record(
            "version",
            {"exact": p["exit"] == 0 and VERSION in p["stdout"] + p["stderr"]},
            [p],
        )
        p = run(["--help"])
        record(
            "help",
            {
                "success": p["exit"] == 0,
                "describes_cli": "--dialect" in p["stdout"] + p["stderr"],
            },
            [p],
        )
        write("good.s", "*=$8000\nstart:\n lda #$05\n rts\n")
        runs = [
            run(["--dialect", "acme", "good.s", "-o", f"good{i}.bin"]) for i in range(2)
        ]
        if corrupt_output:
            (root / "good0.bin").write_bytes(b"wrong bytes")
        expected = bytes.fromhex(
            "a90560"
        )  # Existing CLI fixture; native ACME control retained separately.
        record(
            "deterministic_bytes",
            {
                "success": all(p["exit"] == 0 for p in runs),
                "reference_bytes": bytes_at("good0.bin") == expected,
                "repeat_bytes": bytes_at("good1.bin") == expected,
            },
            runs,
        )
        p = run(
            ["--dialect", "acme", "good.s", "--message-format=json", "-o", "json.bin"]
        )
        obj = parsed(p)
        record(
            "structured_assembly",
            {
                "success": p["exit"] == 0,
                "json_bytes": isinstance(obj, dict)
                and obj.get("bytes") == list(expected),
                "file_bytes": bytes_at("json.bin") == expected,
                "quiet_stderr": not p["stderr"],
            },
            [p],
        )
        # Windows-style line endings and non-ASCII, spaced include paths.
        (root / "include space ü").mkdir()
        write("include space ü/value.s", " lda #$05\r\n rts\r\n")
        write("paths.s", '*=$8000\r\n!source "include space ü/value.s"\r\n')
        p = run(["--dialect", "acme", "paths.s", "-o", "paths.bin"])
        record(
            "unicode_spaces_crlf_include",
            {"success": p["exit"] == 0, "bytes": bytes_at("paths.bin") == expected},
            [p],
        )
        write("bad.s", "*=$8000\n lda #$fff\n")
        human = run(["--dialect", "acme", "bad.s", "-o", "bad.bin"])
        machine = run(
            ["--dialect", "acme", "bad.s", "--message-format=json", "-o", "bad.bin"]
        )
        diagnostics = parsed(machine)
        diag = (
            diagnostics[0]
            if isinstance(diagnostics, list) and len(diagnostics) == 1
            else {}
        )
        message = diag.get("message", "")
        record(
            "invalid_input_diagnostic_parity",
            {
                "both_reject": human["exit"] not in (0, None)
                and machine["exit"] not in (0, None),
                "json_diagnostic": bool(message),
                "same_message": bool(message) and message in human["stderr"],
                "quiet_json_stderr": not machine["stderr"],
                "no_artifact": bytes_at("bad.bin") is None,
            },
            [human, machine],
        )
        write("preserved.bin", "previous output")
        p = run(["--dialect", "acme", "bad.s", "-o", "preserved.bin"])
        record(
            "invalid_input_preserves_output",
            {
                "reject": p["exit"] not in (0, None),
                "preserved": bytes_at("preserved.bin") == b"previous output",
            },
            [p],
        )
        p = run(
            [
                "--dialect",
                "acme",
                "missing.s",
                "--message-format=json",
                "-o",
                "missing.bin",
            ]
        )
        record(
            "missing_input_json",
            {
                "reject": p["exit"] not in (0, None),
                "diagnostic_array": isinstance(parsed(p), list),
                "quiet_stderr": not p["stderr"],
                "no_output": bytes_at("missing.bin") is None,
            },
            [p],
            existing_issue=650,
        )
        write("z80.s", "org $8000\nnop\n")
        for cpu in ["nonsense", "68000"]:
            p = run(["--dialect", "pasmo", "--cpu", cpu, "z80.s", "-o", cpu + ".bin"])
            record(
                "cpu_" + cpu,
                {
                    "reject": p["exit"] not in (0, None),
                    "no_output": bytes_at(cpu + ".bin") is None,
                },
                [p],
                existing_issue=608,
            )
        write("lua.s", " LUA ALLPASS\n sj.add_byte(7)\n ENDLUA\n")
        p = run(["--dialect", "sjasmplus", "lua.s", "-o", "lua.bin"])
        record(
            "release_lua",
            {"success": p["exit"] == 0, "bytes": bytes_at("lua.bin") == b"\x07"},
            [p],
        )
        for flag, issue in [("--prg", 658), ("--org", 663)]:
            args = ["fmt", "--dialect", "acme", "good.s", "-o", f"fmt{issue}.s", flag]
            if flag == "--org":
                args.append("0")
            p = run(args)
            record(
                f"formatter_option_{issue}",
                {
                    "reject": p["exit"] not in (0, None),
                    "no_output": bytes_at(f"fmt{issue}.s") is None,
                },
                [p],
                existing_issue=issue,
            )
        for sidecar in ["--debug=alias.s", "--sym=alias.s", "--listing=alias.s"]:
            write("alias.s", "*=$8000\n lda #$05\n rts\n")
            original = bytes_at("alias.s")
            p = run(["--dialect", "acme", "alias.s", "-o", "alias.bin", sidecar])
            record(
                "source_alias_" + sidecar.split("=")[0],
                {
                    "reject": p["exit"] not in (0, None),
                    "source_preserved": bytes_at("alias.s") == original,
                },
                [p],
                existing_issue=651,
            )
        write("hardlink.s", "*=$8000\n nop\n")
        try:
            os.link(root / "hardlink.s", root / "hardlink.bin")
        except OSError as error:
            results.append(
                {
                    "name": "hardlink_output",
                    "status": "unavailable",
                    "reason": str(error),
                }
            )
        else:
            p = run(["--dialect", "acme", "hardlink.s", "-o", "hardlink.bin"])
            record(
                "hardlink_output",
                {
                    "reject": p["exit"] not in (0, None),
                    "source_preserved": bytes_at("hardlink.s") == b"*=$8000\n nop\n",
                },
                [p],
                existing_issue=651,
            )
        # Complete admitted upstream target: fetch all sources/assets, never run an upstream script.
        project = root / "upstream"
        for name, digest in FILES.items():
            data = verified(
                fetch(
                    f"https://raw.githubusercontent.com/kindjie/6502Assembly/{UPSTREAM}/{name}",
                    cache / "upstream" / name,
                ),
                digest,
            )
            dest = project / name
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(data)
        p = run(
            ["--dialect", "acme", "--prg", "helloworld.asm", "-o", "hello.prg"], project
        )
        output = project / "hello.prg"
        digest = sha(output.read_bytes()) if output.exists() else None
        record(
            "upstream_hello_world",
            {
                "success": p["exit"] == 0,
                "native_acme_bytes": digest
                == "16e5a63289e474e84706517c592c0ba7f2ae86b5278d7d1e6eeb9a0c5ef205fc",
            },
            [p],
            output_sha256=digest,
            expected_size=2081,
            upstream_commit=UPSTREAM,
            source_hashes=FILES,
        )
    counts = {
        state: sum(r["status"] == state for r in results)
        for state in ["pass", "fail", "unavailable"]
    }
    assert len(results) >= 18 and counts["pass"] >= 5, "audit controls did not run"
    for name in ["version", "deterministic_bytes", "structured_assembly"]:
        assert next(r for r in results if r["name"] == name)["status"] == "pass", (
            f"positive control failed: {name}"
        )
    return {
        "version": VERSION,
        "source": SOURCE,
        "probe_sha256": sha(Path(__file__).read_bytes()),
        "target": target,
        "archive": archive,
        "archive_sha256": ARCHIVES[target],
        "executable_sha256": sha(executable),
        "host": {
            "system": platform.system(),
            "release": platform.release(),
            "machine": platform.machine(),
            "python": sys.version,
            "github_run_id": os.environ.get("GITHUB_RUN_ID"),
            "runner_os": os.environ.get("RUNNER_OS"),
            "runner_arch": os.environ.get("RUNNER_ARCH"),
            "libc": platform.libc_ver(),
        },
        "checksum_mutation_rejected": checksum_control,
        "counts": counts,
        "qualification_passed": counts["fail"] == 0 and counts["unavailable"] == 0,
        "results": results,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=ARCHIVES, required=True)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--control-corrupt-output", action="store_true")
    args = parser.parse_args()
    if sys.version_info < (3, 13):
        raise SystemExit("Use Python 3.13+")
    report = audit(
        args.target, args.cache.resolve(), args.binary, args.control_corrupt_output
    )
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(
        json.dumps(
            {
                "target": report["target"],
                "counts": report["counts"],
                "qualification_passed": report["qualification_passed"],
            }
        )
    )


if __name__ == "__main__":
    main()
