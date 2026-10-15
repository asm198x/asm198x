#!/usr/bin/env python3
"""Verify retained audit integrity and nonempty result invariants, without rerunning hosts."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


def verify(root: Path) -> None:
    manifest = json.loads((root / "manifest.json").read_text())
    assert manifest and len(manifest) >= 15, "missing evidence manifest"
    for name, expected in manifest.items():
        assert Path(name).name == name, "unsafe manifest path"
        data = (root / name).read_bytes()
        assert (
            len(data) == expected["size"]
            and hashlib.sha256(data).hexdigest() == expected["sha256"]
        ), name
    inventory = json.loads((root / "archive-inventory.json").read_text())
    assets = inventory["assets"]
    assert len(assets) == 5 and len({a["archive"] for a in assets}) == 5
    probe = (root / "probe.py").read_bytes().replace(b"\r\n", b"\n")
    probe_hashes = {
        hashlib.sha256(probe).hexdigest(),
        hashlib.sha256(probe.replace(b"\n", b"\r\n")).hexdigest(),
    }
    reports = [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "x86_64-pc-windows-msvc",
        "x86_64-unknown-linux-gnu",
        "linux-arm64",
        "macos-arm64",
    ]
    failures = {
        "missing_input_json",
        "cpu_nonsense",
        "cpu_68000",
        "formatter_option_658",
        "formatter_option_663",
        "hardlink_output",
    }
    names: set[str] | None = None
    for stem in reports:
        report = json.loads((root / f"{stem}.json").read_text())
        assert report["version"] == "0.0.65" and report["source"] == inventory["source"]
        assert report["probe_sha256"] in probe_hashes
        asset = next(a for a in assets if a["archive"] == report["archive"])
        assert report["archive_sha256"] == asset["sha256"]
        assert report["executable_sha256"] == asset["executable_sha256"]
        results = report["results"]
        current_names = {r["name"] for r in results}
        assert len(results) == len(current_names) == 19, "missing/duplicate cases"
        if names is None:
            names = current_names
        assert current_names == names
        for result in results:
            if result["status"] == "unavailable":
                assert result["name"] == "published_installer" and stem == "linux-arm64"
                assert result["reason"]
            else:
                checks = result["checks"]
                assert checks and all(type(value) is bool for value in checks.values())
                assert result["status"] == ("pass" if all(checks.values()) else "fail")
        counts = {
            state: sum(r["status"] == state for r in results)
            for state in ["pass", "fail", "unavailable"]
        }
        assert counts == report["counts"] and counts["pass"] >= 12, (
            "invalid result counts"
        )
        assert {r["name"] for r in results if r["status"] == "fail"} == failures
        assert report["qualification_passed"] is False
        assert report["checksum_mutation_rejected"] is True
    controls = json.loads((root / "controls.json").read_text())
    assert (
        controls["exit"] != 0
        and controls["corrupt_output_rejected"]
        and controls["setup_wrong_version_rejected"]
    )
    assert "positive control failed: deterministic_bytes" in controls["stderr"]
    api = json.loads((root / "npm-api.json").read_text())
    assert api["package"] == "@asm198x/z80" and api["version"] == "0.4.0"
    assert len(api["checks"]) == 7 and all(api["checks"].values())
    print(
        "Verified six host observations, five release archives, seven npm API checks and retained controls"
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--directory", type=Path, default=Path(__file__).resolve().parent
    )
    args = parser.parse_args()
    verify(args.directory)


if __name__ == "__main__":
    main()
