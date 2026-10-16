"""Observe existing conversion contracts; findings are not qualification passes."""

import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

BASIC = " org 8000h\nstart: ld a,5\n ret\n"
REPEAT = " org 8000h\n rept 2 ; repeat start\n nop ; body\n endm ; repeat end\n ret\n"
CONDITIONAL = " org 8000h\n if 1 ; if start\n ld a,5\n else ; else marker\n ld a,6\n endif ; if end\n ret\n"
MACRO = " org 8000h\nsetv macro\n ld a,5\n endm\n setv\n ret\n"
NESTED = " org 8000h\nsetv macro\n rept 2\n nop\n endm\n endm\n setv\n"
CASES = [
    ("basic_stdout", BASIC, [], "success"),
    ("basic_new", BASIC, ["-o", "out.asm"], "success"),
    ("basic_existing", BASIC, ["-o", "out.asm"], "success"),
    ("inplace", BASIC, ["-o", "input.asm"], "observation"),
    ("hardlink", BASIC, ["-o", "alias.asm"], "observation"),
    ("symlink", BASIC, ["-o", "alias.asm"], "observation"),
    ("repeat_comments", REPEAT, [], "success"),
    ("conditional_comments", CONDITIONAL, [], "success"),
    ("macro", MACRO, [], "success"),
    ("nested_macro_repeat", NESTED, ["-o", "out.asm"], "defect"),
    (
        "include_present",
        ' org 8000h\n include "child.inc"\n ret\n',
        ["-o", "out.asm"],
        "limitation",
    ),
    (
        "incbin_present",
        ' org 8000h\n incbin "data.bin"\n ret\n',
        ["-o", "out.asm"],
        "limitation",
    ),
    ("invalid_syntax", " org 8000h\n xyzzy 1\n", ["-o", "out.asm"], "rejection"),
    ("unsupported_pair", BASIC, ["-o", "out.asm"], "rejection"),
    (
        "unsupported_option",
        BASIC,
        ["--message-format=json", "-o", "out.asm"],
        "rejection",
    ),
    ("missing_input", BASIC, ["-o", "out.asm"], "rejection"),
    ("output_directory", BASIC, ["-o", "out.asm"], "rejection"),
    ("pasmonext_base", BASIC, [], "success"),
    ("pasmonext_extended", " org 8000h\n swapnib\n ret\n", [], "success"),
    (
        "include_missing",
        ' org 8000h\n include "missing.inc"\n',
        ["-o", "out.asm"],
        "rejection",
    ),
]


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def snapshot(root):
    return {
        str(p.relative_to(root)): {
            "hex": p.read_bytes().hex(),
            "symlink": os.readlink(p) if p.is_symlink() else None,
        }
        for p in sorted(root.rglob("*"))
        if p.is_file()
    }


def process(command, root):
    r = subprocess.run(command, cwd=root, capture_output=True, timeout=30, check=False)
    return {
        "argv": command,
        "exit": r.returncode,
        "stdout": r.stdout.decode(),
        "stderr": r.stderr.decode(),
    }


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def run(executable, output):
    asm = str(Path(executable).resolve())
    pasmo = shutil.which("pasmo")
    sjasm = shutil.which("sjasmplus")
    require(pasmo and sjasm, "both native reference tools are required")
    observations = []
    for repetition in [1, 2]:
        for name, source, extra, expected in CASES:
            with tempfile.TemporaryDirectory(
                prefix="conversion-", dir=os.environ.get("TMPDIR")
            ) as temporary:
                root = Path(temporary)
                (root / "input.asm").write_text(source)
                (root / "child.inc").write_text(" db 42\n")
                (root / "data.bin").write_bytes(b"\x2a")
                if name in {
                    "basic_existing",
                    "nested_macro_repeat",
                    "include_present",
                    "incbin_present",
                    "invalid_syntax",
                    "unsupported_pair",
                    "unsupported_option",
                    "missing_input",
                    "include_missing",
                }:
                    (root / "out.asm").write_text("previous source\n")
                if name == "hardlink":
                    os.link(root / "input.asm", root / "alias.asm")
                if name == "symlink":
                    (root / "alias.asm").symlink_to("input.asm")
                if name == "output_directory":
                    (root / "out.asm").mkdir()
                before = snapshot(root)
                from_name = "pasmonext" if name.startswith("pasmonext") else "pasmo"
                pair = (
                    ["--from", "acme", "--to", "ca65"]
                    if name == "unsupported_pair"
                    else ["--from", from_name, "--to", "sjasmplus"]
                )
                result = process(
                    [
                        asm,
                        "convert",
                        *pair,
                        "missing.asm" if name == "missing_input" else "input.asm",
                        *extra,
                    ],
                    root,
                )
                after = snapshot(root)
                references = []
                if expected in {"success", "observation"}:
                    require(
                        result["exit"] == 0,
                        f"positive success control failed: {name}: {result}",
                    )
                    destination = extra[1] if extra[:1] == ["-o"] else None
                    converted = (
                        (root / destination).read_text()
                        if destination
                        else result["stdout"]
                    )
                    require(bool(converted.strip()), f"empty converted source: {name}")
                    require(
                        "verified byte-identical" in result["stderr"],
                        f"missing verification claim: {name}",
                    )
                    if destination:
                        require(
                            not result["stdout"],
                            f"file conversion leaked stdout: {name}",
                        )
                    if expected == "success":
                        require(
                            after["input.asm"] == before["input.asm"],
                            f"source changed: {name}",
                        )
                    for marker in (
                        ["repeat start", "body", "repeat end"]
                        if name == "repeat_comments"
                        else ["if start", "else marker", "if end"]
                        if name == "conditional_comments"
                        else []
                    ):
                        require(marker in converted, f"lost comment {marker}: {name}")
                    (root / "reference-input.asm").write_text(source)
                    (root / "converted.asm").write_text(converted)
                    reference_source = process(
                        [pasmo, "--bin", "reference-input.asm", "reference.bin"], root
                    )
                    target_args = ["--zxnext"] if name == "pasmonext_extended" else []
                    reference_target = process(
                        [sjasm, *target_args, "--raw=target.bin", "converted.asm"], root
                    )
                    references.extend([reference_source, reference_target])
                    require(
                        reference_source["exit"] == reference_target["exit"] == 0,
                        f"native reference failed: {name}: {references}",
                    )
                    source_bytes = (root / "reference.bin").read_bytes()
                    target_bytes = (root / "target.bin").read_bytes()
                    require(
                        source_bytes == target_bytes and source_bytes,
                        f"native byte parity failed: {name}",
                    )
                    expected_bytes = {
                        "basic_stdout": "3e05c9",
                        "repeat_comments": "0000c9",
                        "pasmonext_extended": "ed23c9",
                    }
                    if name in expected_bytes:
                        require(
                            source_bytes.hex() == expected_bytes[name],
                            f"wrong positive fixture: {name}",
                        )
                    binary = source_bytes.hex()
                    if name == "pasmonext_extended":
                        unconfigured = process(
                            [sjasm, "--raw=unconfigured.bin", "converted.asm"], root
                        )
                        require(
                            unconfigured["exit"] != 0,
                            "unconfigured Next target unexpectedly accepted",
                        )
                        references.append(unconfigured)
                else:
                    require(
                        result["exit"] != 0 and not result["stdout"],
                        f"rejection failed: {name}: {result}",
                    )
                    require(
                        before == after, f"rejected conversion changed files: {name}"
                    )
                    binary = None
                    if expected in {"limitation", "defect"}:
                        native = process(
                            [pasmo, "--bin", "input.asm", "reference.bin"], root
                        )
                        references.append(native)
                        require(
                            native["exit"] == 0,
                            f"limitation fixture is not native-valid: {name}: {native}",
                        )
                        native_assembly = process(
                            [
                                asm,
                                "--dialect",
                                "pasmo",
                                "input.asm",
                                "-o",
                                "ordinary.bin",
                            ],
                            root,
                        )
                        references.append(native_assembly)
                        binary = (root / "reference.bin").read_bytes().hex()
                        if name == "nested_macro_repeat":
                            require(
                                binary == "0000" and native_assembly["exit"] != 0,
                                "nested macro comparison changed",
                            )
                            target_source = source.replace(
                                " nop\n endm\n", " nop\n endr\n"
                            )
                            (root / "native-target.asm").write_text(target_source)
                            target = process(
                                [sjasm, "--raw=native-target.bin", "native-target.asm"],
                                root,
                            )
                            references.append(target)
                            require(
                                target["exit"] == 0
                                and (root / "native-target.bin").read_bytes().hex()
                                == binary,
                                "native nested target does not match",
                            )
                        if name in {"include_present", "incbin_present"}:
                            require(
                                native_assembly["exit"] == 0,
                                f"ordinary include control failed: {name}",
                            )
                            require(
                                (root / "ordinary.bin").read_bytes()
                                == (root / "reference.bin").read_bytes()
                                == bytes([42, 0xC9]),
                                f"include control mismatch: {name}",
                            )
                observations.append(
                    {
                        "name": name,
                        "repetition": repetition,
                        "classification": expected,
                        "source": source,
                        "before": before,
                        "result": result,
                        "after": after,
                        "native_binary_hex": binary,
                        "references": references,
                        "reference_files": snapshot(root),
                    }
                )
    require(len(observations) == 40, "missing observations")
    for left, right in zip(observations[:20], observations[20:], strict=True):
        for key in [
            "name",
            "classification",
            "source",
            "before",
            "result",
            "after",
            "native_binary_hex",
        ]:
            require(left[key] == right[key], f"unstable {key}: {left['name']}")
    clean = (
        subprocess.run(
            [
                "git",
                "diff",
                "--quiet",
                "HEAD",
                "--",
                "crates",
                "Cargo.toml",
                "Cargo.lock",
            ],
            check=False,
        ).returncode
        == 0
    )
    require(clean, "tracked production source changed during audit")
    identity = {
        "tracked_production_clean": clean,
        "cargo_lock_sha256": sha("Cargo.lock"),
        "rustc": subprocess.run(
            ["rustc", "--version"], check=True, capture_output=True, text=True
        ).stdout.strip(),
        "build_command": ["cargo", "build", "-p", "asm198x", "--bin", "asm198x"],
        "source": subprocess.run(
            ["git", "rev-parse", "HEAD"], check=True, capture_output=True, text=True
        ).stdout.strip(),
        "platform": platform.platform(),
        "python": platform.python_version(),
        "tools": {
            label: {
                "path": str(Path(path).resolve()),
                "sha256": sha(path),
                "version_process": process([path, *args], Path.cwd()),
            }
            for label, path, args in [
                ("asm198x", asm, ["--version"]),
                ("pasmo", pasmo, []),
                ("sjasmplus", sjasm, ["--version"]),
            ]
        },
    }
    Path(output).write_text(
        json.dumps({"identity": identity, "observations": observations}, indent=2)
        + "\n"
    )
    print(
        "Retained 40 repeated observations; native controls and expected rejections passed"
    )


if __name__ == "__main__":
    run(*sys.argv[1:])
