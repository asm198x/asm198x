"""Verify retained audit coverage and bytes without running product/reference tools."""

import hashlib
import json
import sys
from pathlib import Path


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def verify(root):
    manifest = json.loads((root / "manifest.json").read_text())
    expected = {
        "probe.py",
        "verify.py",
        "observations.json",
        "controls.json",
        "report.md",
    }
    require(set(manifest) == expected, "incomplete manifest")
    for name, digest in manifest.items():
        require(
            hashlib.sha256((root / name).read_bytes()).hexdigest() == digest,
            f"hash mismatch: {name}",
        )
    data = json.loads((root / "observations.json").read_text())
    rows = data["observations"]
    require(bool(rows), "missing/empty observations")
    require(len(rows) == 40, "missing observations")
    categories = {
        "success": {
            "basic_stdout",
            "basic_new",
            "basic_existing",
            "repeat_comments",
            "conditional_comments",
            "macro",
            "pasmonext_base",
            "pasmonext_extended",
        },
        "observation": {"inplace", "hardlink", "symlink"},
        "defect": {"nested_macro_repeat"},
        "limitation": {"include_present", "incbin_present"},
        "rejection": {
            "invalid_syntax",
            "unsupported_pair",
            "unsupported_option",
            "missing_input",
            "output_directory",
            "include_missing",
        },
    }
    names = set().union(*categories.values())
    for repetition in [1, 2]:
        group = [row for row in rows if row["repetition"] == repetition]
        require(
            len(group) == 20 and {row["name"] for row in group} == names,
            "case coverage mismatch",
        )
        for row in group:
            name = row["name"]
            kind = row["classification"]
            require(name in categories[kind], "wrong classification")
            result = row["result"]
            before, after = row["before"], row["after"]
            references = row["references"]
            files = row["reference_files"]
            if kind in {"success", "observation"}:
                require(
                    result["exit"] == 0
                    and "verified byte-identical" in result["stderr"],
                    "missing positive result",
                )
                require(
                    references[0]["exit"] == references[1]["exit"] == 0,
                    "native positive control failed",
                )
                require(
                    files["reference.bin"]["hex"]
                    == files["target.bin"]["hex"]
                    == row["native_binary_hex"]
                    and bool(row["native_binary_hex"]),
                    "native parity missing",
                )
                if kind == "success":
                    require(
                        before["input.asm"] == after["input.asm"],
                        "source unexpectedly changed",
                    )
                else:
                    destination = "input.asm" if name == "inplace" else "alias.asm"
                    require(
                        after["input.asm"]["hex"] == after[destination]["hex"]
                        and after["input.asm"]["hex"] != before["input.asm"]["hex"],
                        "alias observation missing",
                    )
                if name == "repeat_comments":
                    require(
                        all(
                            marker in result["stdout"]
                            for marker in ["repeat start", "body", "repeat end"]
                        ),
                        "repeat comment missing",
                    )
                if name == "conditional_comments":
                    require(
                        all(
                            marker in result["stdout"]
                            for marker in ["if start", "else marker", "if end"]
                        ),
                        "conditional comment missing",
                    )
                if name == "pasmonext_extended":
                    require(
                        row["native_binary_hex"] == "ed23c9"
                        and references[2]["exit"] != 0
                        and "--zxnext" in references[1]["argv"],
                        "Next target control missing",
                    )
            else:
                require(
                    result["exit"] != 0 and not result["stdout"] and before == after,
                    "rejection/output preservation failed",
                )
            if kind in {"defect", "limitation"}:
                require(references[0]["exit"] == 0, "native validity missing")
                if kind == "defect":
                    require(
                        row["native_binary_hex"] == "0000"
                        and references[1]["exit"] != 0
                        and references[2]["exit"] == 0,
                        "nested comparison missing",
                    )
                    require(
                        files["native-target.bin"]["hex"] == "0000"
                        and "line 6" in result["stderr"],
                        "nested evidence missing",
                    )
                else:
                    require(
                        references[1]["exit"] == 0
                        and files["ordinary.bin"]["hex"]
                        == files["reference.bin"]["hex"]
                        == "2ac9",
                        "ordinary include comparison missing",
                    )
                    require(
                        "single-source API" in result["stderr"]
                        and "automatically" in result["stderr"],
                        "include diagnostic missing",
                    )
    for first, second in zip(rows[:20], rows[20:], strict=True):
        for key in [
            "name",
            "classification",
            "source",
            "before",
            "result",
            "after",
            "native_binary_hex",
            "reference_files",
        ]:
            require(
                first[key] == second[key],
                f"unstable repeated outcome: {first['name']}/{key}",
            )
    identity = data["identity"]
    require(
        identity["tracked_production_clean"]
        and len(identity["cargo_lock_sha256"]) == 64
        and identity["rustc"].startswith("rustc 1.98.0"),
        "missing clean build identity",
    )
    require(
        identity["source"] == "c96f2b89e474675a0f9f8d199907fd7d7f505011",
        "unexpected binary source",
    )
    require(
        set(identity["tools"]) == {"asm198x", "pasmo", "sjasmplus"},
        "missing tool identities",
    )
    for tool in identity["tools"].values():
        require(
            len(tool["sha256"]) == 64 and bool(tool["version_process"]["stderr"]),
            "missing executable identity",
        )
    controls = json.loads((root / "controls.json").read_text())
    require(
        controls["failing_executable"]["exit"] != 0
        and not controls["failing_executable"]["output_created"],
        "failed executable accepted",
    )
    require(
        "positive success control failed: basic_stdout"
        in controls["failing_executable"]["error"],
        "false probe control missing",
    )
    require(
        controls["empty_observations"]
        == {"exit": 1, "error": "missing/empty observations"},
        "empty-evidence control missing",
    )
    print(
        "Retained audit verified: 40 observations, native byte parity, rejected-input preservation and failing controls"
    )


if __name__ == "__main__":
    verify(Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).parent)
