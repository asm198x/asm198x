"""Verify the retained 0.0.66 audit without executing a host or reference tool."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path


def verify(root: Path) -> None:
    expected = {
        "identity.json",
        "observations.json",
        "origin-api-control.json",
        "documentation-claims.json",
        "probe.py",
        "report.md",
        "verification-controls.json",
        "verify.py",
    }
    manifest = json.loads((root / "manifest.json").read_text())
    assert set(manifest) == expected
    for name, digest in manifest.items():
        assert hashlib.sha256((root / name).read_bytes()).hexdigest() == digest, name
    identity = json.loads((root / "identity.json").read_text())
    report = json.loads((root / "observations.json").read_text())
    assert identity["version"] == report["version"] == "0.0.66"
    assert (
        identity["binary_source_commit"]
        == report["source_commit"]
        == "35a07893a57cbb42c3a341cc293fc4b713af99f4"
    )
    assert identity["executable_sha256"] == report["binary_sha256"]
    assert (
        identity["target"] == "aarch64-apple-darwin"
        and identity["existing_archive_gate_passed"] is True
    )
    observations = report["observations"]
    assert len(observations) == 46, "missing/empty observations"
    controls = {
        "linear_6502",
        "linear_z80",
        "linear_68000",
        "valid_assembly",
        "invalid_cpu",
        "invalid_sidecar",
        "recovery_new",
        "recovery_existing",
        "format_acme",
    }
    flags = {
        "output_short",
        "output_long",
        "container_prg",
        "container_sna",
        "container_tap",
        "container_tzx",
        "container_exe",
        "container_gb",
        "linker_config",
        "outprefix",
        "equ",
        "include_dir",
        "json",
    }
    names = controls | flags | {"origin_68000_high"}
    assert len(names) == 23
    for repeat in [0, 1]:
        records = [r for r in observations if r["repeat"] == repeat]
        assert len(records) == 23 and {r["name"] for r in records} == names
        baseline = next(r["process"] for r in records if r["name"] == "linear_6502")
        for record in records:
            name = record["name"]
            assert all(record["after"].get(k) == v for k, v in record["before"].items())
            if name in flags:
                assert (
                    record["classification"] == "violation"
                    and record["process"]["exit"] == 0
                )
                assert record["before"] == record["after"]
                for stream in ["stdout", "stderr"]:
                    assert record["process"][stream] == baseline[stream]
            elif name == "origin_68000_high":
                assert (
                    record["classification"] == "limitation"
                    and record["process"]["exit"] == 1
                )
                assert "invalid address `0x10000`" in record["process"]["stderr"]
            else:
                assert record["classification"] == "pass"
    normalize = lambda rows: [
        {k: v for k, v in r.items() if k != "repeat"} for r in rows
    ]
    assert normalize(observations[:23]) == normalize(observations[23:])
    api = json.loads((root / "origin-api-control.json").read_text())
    assert api["exit"] == 0 and api["lock_matches_release"] is True
    assert "bra.s $8004" in api["stdout"] and "bra.s $10004" in api["stdout"]
    claims = json.loads((root / "documentation-claims.json").read_text())
    assert "otherwise Z80" in claims["disassembly_claim"]
    assert claims["help_formatter_claim"] in report["help"]["stderr"]
    controls_data = json.loads((root / "verification-controls.json").read_text())
    assert controls_data["empty_observations"] == {
        "rejected": True,
        "reason": "missing/empty observations",
        "manifest_rehashed": True,
    }
    bad = controls_data["invalid_executable"]
    assert bad["exit"] != 0 and bad["rejected"] is True
    assert "positive success control failed: linear_6502" in bad["stderr"]
    print(
        "Verified 46 repeated observations, nine controls, three findings and retained hashes."
    )


if __name__ == "__main__":
    verify(Path(__file__).resolve().parent)
