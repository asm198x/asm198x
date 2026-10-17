"""Check complete SpecNext bytes, process streams and displayed addresses."""

import argparse
import hashlib
import json
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("result", type=Path)
    parser.add_argument("producer")
    args = parser.parse_args()
    report = json.loads((args.result / "report.json").read_text())
    record = report["records"][0]
    assert report["assembler"]["source_commit"] == args.producer
    assert report["selected"] == ["howprice-specnext-invaders/game"]
    assert record["source_commit"] == "e3211ea8ad2e83aca4d07d70bd8e09b7b51a9a1d"
    assert record["outcome"] == "equivalent"
    prefix = "howprice-specnext-invaders--game-"
    processes = {
        "build-assembler": report["assembler"]["build"],
        "assembler-version": report["assembler"]["version"],
        prefix + "version-sjasmplus": record["tools"]["sjasmplus"]["version"],
        prefix + "reference-0": record["reference_steps"][0]["process"],
        prefix + "assembler-0": record["assembler_steps"][0]["process"],
    }
    for name, process in processes.items():
        assert process["exit"] == 0 and process["failure"] is None
        for stream in ["stdout", "stderr"]:
            assert (args.result / "logs" / f"{name}.{stream}").read_text() == process[
                stream
            ]
    comparison = record["comparisons"][0]
    assert comparison["equal"] is True
    for side in ["reference", "assembler"]:
        artifact = comparison[side]
        data = bytes.fromhex(artifact["hex"])
        assert len(data) == artifact["length"] == 33280
        assert (
            hashlib.sha256(data).hexdigest()
            == artifact["sha256"]
            == "5fbed92a7d60dab47de8b22162c9227df051eaf67c26e85e618b24f6429dd07f"
        )
    assert comparison["reference"]["hex"] == comparison["assembler"]["hex"]
    reference_notes = [
        line[2:]
        for line in processes[prefix + "reference-0"]["stderr"].splitlines()
        if line.startswith("> ")
    ]
    assembler_notes = [
        line.split(": note: ", 1)[1]
        for line in processes[prefix + "assembler-0"]["stderr"].splitlines()
        if ": note: " in line
    ]
    assert len(reference_notes) == 14
    assert assembler_notes == reference_notes
    print("Verified both NEX images, 14 displayed values and ten complete streams")


if __name__ == "__main__":
    main()
