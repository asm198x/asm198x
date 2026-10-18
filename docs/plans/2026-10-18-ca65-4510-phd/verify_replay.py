"""Verify the clean 4510 target replay and complete byte identity."""

import argparse
import hashlib
import json
from pathlib import Path


def verify_artifact(artifact: dict) -> bytes:
    data = bytes.fromhex(artifact["hex"])
    assert len(data) == artifact["length"] == 564
    assert (
        hashlib.sha256(data).hexdigest()
        == artifact["sha256"]
        == "8a5fcde630e55dd49403f7c6ffed25d8786e65e3d0a2723531b7ca3e2c0509ec"
    )
    return data


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("result", type=Path)
    parser.add_argument("--producer", required=True)
    args = parser.parse_args()
    report = json.loads((args.result / "report.json").read_text())
    assert report["assembler"]["source_commit"] == args.producer
    assert report["selected"] == ["cc65-4510-opcodes/opcode-space"]
    record = report["records"][0]
    assert record["source_commit"] == "e11fb5c39371046ebe25485f984f644c5a0d65d3"
    assert record["outcome"] == "equivalent"
    prefix = "cc65-4510-opcodes--opcode-space-"
    processes = {
        "build-assembler": report["assembler"]["build"],
        "assembler-version": report["assembler"]["version"],
    }
    for name, tool in record["tools"].items():
        processes[prefix + "version-" + name] = tool["version"]
    for side in ["reference", "assembler"]:
        for index, step in enumerate(record[side + "_steps"]):
            processes[prefix + side + "-" + str(index)] = step["process"]
    assert len(processes) == 7
    for name, process in processes.items():
        for stream in ["stdout", "stderr"]:
            assert (args.result / "logs" / f"{name}.{stream}").read_text() == process[
                stream
            ]
        assert process["exit"] == 0
        assert process["failure"] is None
    artifact = record["reference_steps"][-1]["outputs"][0]
    data = verify_artifact(artifact)
    assembled = record["assembler_steps"][-1]["outputs"][0]
    assert verify_artifact(assembled) == data
    assert record["comparisons"]
    # A changed byte must fail the same content identity check.
    changed = bytearray(data)
    changed[-1] ^= 1
    corrupt = dict(artifact, hex=changed.hex())
    try:
        verify_artifact(corrupt)
    except AssertionError:
        pass
    else:
        raise AssertionError("corrupted native bytes passed verification")
    print(
        "Verified native 564-byte reference, complete Asm/native byte identity and fourteen streams; changed byte rejected"
    )


if __name__ == "__main__":
    main()
