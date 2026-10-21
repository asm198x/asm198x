"""Replay all retained matrices against a losslessly stored checked predecessor."""

import argparse
import gzip
import hashlib
import json
import re
import subprocess
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[3]


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--before-format", required=True)
    parser.add_argument("--expected-matches", type=int, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    binary_hash = digest(binary.read_bytes())
    baseline = args.baseline.resolve(strict=True)
    manifest = args.manifest.resolve(strict=True)
    records = json.loads(manifest.read_text())["files"]
    index = {row["path"]: row for row in records}
    assert len(index) == len(records)
    matrices = json.loads((baseline / "replay-summary.json").read_text())
    assert len(matrices) == len({row["label"] for row in matrices}) == 39
    assert sum(row["cases"] for row in matrices) == 7309
    args.output.mkdir(exist_ok=False)
    summary: list[dict[str, Any]] = []
    for row in matrices:
        label = row["label"]
        assert re.fullmatch(r"[a-zA-Z0-9_-]+", label)
        facts = Path(row["facts"]).resolve(strict=True)
        assert facts.is_relative_to(baseline)
        assert digest(facts.read_bytes()) == row["facts_sha256"]
        stored = index[args.before_format.format(label=label)]
        assert stored["blob"] == stored["sha256"] + ".gz"
        before_bytes = gzip.decompress((manifest.parent / stored["blob"]).read_bytes())
        assert digest(before_bytes) == stored["sha256"]
        before_rows = json.loads(before_bytes)["results"]
        before = {result["name"]: result for result in before_rows}
        assert len(before) == len(before_rows) == row["cases"]
        result = args.output / (label + ".json")
        if row["comparator"] == "empty-calls":
            command = [
                "python3",
                str(ROOT / "docs/plans/2026-10-21-ca65-empty-calls/compare.py"),
                str(binary),
                str(facts),
                str(result),
            ]
        else:
            assert row["comparator"] == "consumer"
            command = [
                "python3",
                str(
                    ROOT
                    / "docs/plans/2026-10-20-ca65-string-token-provenance/consumer_compare.py"
                ),
                str(binary),
                str(result),
                "--facts",
                str(facts),
                *(["--flat"] if row["flat"] else []),
            ]
        with (args.output / (label + ".log")).open("w") as log:
            subprocess.run(
                command, check=True, timeout=180, stdout=log, stderr=subprocess.STDOUT
            )
        after = json.loads(result.read_text())["results"]
        assert len(after) == len({item["name"] for item in after}) == row["cases"]
        assert {item["name"] for item in after} == before.keys()
        lost = [
            item["name"]
            for item in after
            if before[item["name"]]["matches_native"] and not item["matches_native"]
        ]
        assert not lost, (label, lost)
        summary.append(
            {
                "label": label,
                "cases": len(after),
                "before_matches": sum(
                    item["matches_native"] for item in before.values()
                ),
                "after_matches": sum(item["matches_native"] for item in after),
                "lost_matches": lost,
            }
        )
        print(label, len(after), summary[-1]["after_matches"], flush=True)
    assert digest(binary.read_bytes()) == binary_hash
    assert sum(row["before_matches"] for row in summary) == args.expected_matches > 0
    (args.output / "summary.json").write_text(
        json.dumps(
            {
                "binary_sha256": binary_hash,
                "predecessor_manifest": str(manifest),
                "expected_matches": args.expected_matches,
                "matrices": summary,
            },
            indent=2,
        )
        + "\n"
    )
    print("All checked predecessor matches retained", flush=True)


if __name__ == "__main__":
    main()
