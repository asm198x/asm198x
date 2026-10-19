"""Verify the complete pinned SpecNext NEX replay, streams and corruption guard."""

import argparse
import hashlib
import json
from pathlib import Path

NEX_SHA256 = "5fbed92a7d60dab47de8b22162c9227df051eaf67c26e85e618b24f6429dd07f"


def artifact_bytes(artifact: dict) -> bytes:
    data = bytes.fromhex(artifact["hex"])
    assert len(data) == artifact["length"] > 0
    assert hashlib.sha256(data).hexdigest() == artifact["sha256"]
    return data


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("result", type=Path)
    parser.add_argument("--producer", required=True)
    args = parser.parse_args()
    report = json.loads((args.result / "report.json").read_text())
    assert report["assembler"]["source_commit"] == args.producer
    assert report["selected"] == ["howprice-specnext-invaders/game"]
    assert len(report["records"]) == 1
    record = report["records"][0]
    assert record["source_commit"] == "e3211ea8ad2e83aca4d07d70bd8e09b7b51a9a1d"
    assert record["outcome"] == "equivalent"
    prefix = "howprice-specnext-invaders--game-"
    processes = {
        "build-assembler": report["assembler"]["build"],
        "assembler-version": report["assembler"]["version"],
    }
    for name, tool in record["tools"].items():
        processes[prefix + "version-" + name] = tool["version"]
    for side in ["reference", "assembler"]:
        assert len(record[side + "_steps"]) == 1
        for index, step in enumerate(record[side + "_steps"]):
            processes[prefix + side + "-" + str(index)] = step["process"]
            for artifact in step["outputs"]:
                if artifact["hex"] is not None:
                    artifact_bytes(artifact)
                else:
                    assert artifact["length"] > 0 and len(artifact["sha256"]) == 64
    assert len(processes) == 5
    for name, process in processes.items():
        assert process["exit"] == 0 and process["failure"] is None
        for stream in ["stdout", "stderr"]:
            assert (args.result / "logs" / f"{name}.{stream}").read_text() == process[
                stream
            ]
    assert len(record["comparisons"]) == 1
    comparison = record["comparisons"][0]
    assert comparison["equal"] is True
    reference = artifact_bytes(comparison["reference"])
    assembled = artifact_bytes(comparison["assembler"])
    assert reference == assembled and len(reference) == 33280
    assert reference[:4] == b"Next"
    assert (
        comparison["reference"]["sha256"]
        == comparison["assembler"]["sha256"]
        == NEX_SHA256
    )
    for side in ["reference", "assembler"]:
        artifact = next(
            a
            for a in record[side + "_steps"][0]["outputs"]
            if a["path"] == "bin/invaders.nex"
        )
        assert artifact["length"] == len(reference)
        assert artifact["sha256"] == NEX_SHA256
        if artifact["hex"] is not None:
            assert artifact_bytes(artifact) == reference
    changed = bytearray(assembled)
    changed[-1] ^= 1
    corrupt = dict(comparison["assembler"], hex=changed.hex())
    try:
        artifact_bytes(corrupt)
    except AssertionError:
        pass
    else:
        raise AssertionError("changed-byte control was accepted")
    print(
        "Verified all 33,280 native NEX bytes, five successful processes and ten streams; changed byte rejected"
    )


if __name__ == "__main__":
    main()
