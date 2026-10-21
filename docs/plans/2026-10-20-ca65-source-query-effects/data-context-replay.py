"""Replay every retained baseline dataset and reject any lost native match."""

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    args.output.mkdir(exist_ok=False)
    summary = []
    root = Path(__file__).parent
    for row in json.loads((args.baseline / "replay-summary.json").read_text()):
        label = row["label"]
        prior = args.baseline / (label + "-after.json")
        if label == "conditional-context":
            prior = args.baseline / "conditional-after.json"
        before = json.loads(prior.read_text())["results"]
        output = args.output / (label + "-after.json")
        command = [
            sys.executable,
            str(
                root.parent
                / "2026-10-20-ca65-string-token-provenance/consumer_compare.py"
            ),
            str(args.binary),
            str(output),
            "--facts",
            row["facts"],
        ]
        if label == "strings-flat-native":
            command.append("--flat")
        result = subprocess.run(
            command,
            capture_output=True,
            text=True,
            timeout=180,
            check=False,
            env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
        )
        (args.output / (label + ".log")).write_text(result.stdout + result.stderr)
        if result.returncode:
            raise RuntimeError(f"{label}: {result.returncode}: {result.stderr}")
        after = json.loads(output.read_text())["results"]
        assert len(before) == len(after) == row["cases"] > 0, label
        now = {r["name"]: r["matches_native"] for r in after}
        assert len(now) == len(after), label
        lost = [r["name"] for r in before if r["matches_native"] and not now[r["name"]]]
        summary.append(
            {
                "label": label,
                "facts": row["facts"],
                "baseline": str(prior),
                "cases": len(after),
                "before_matches": sum(r["matches_native"] for r in before),
                "after_matches": sum(r["matches_native"] for r in after),
                "lost_matches": lost,
            }
        )
        print(label, len(after), "lost", lost, flush=True)
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    assert len(summary) == 23 and sum(r["cases"] for r in summary) == 2466
    assert not any(r["lost_matches"] for r in summary), "native matches regressed"


if __name__ == "__main__":
    main()
