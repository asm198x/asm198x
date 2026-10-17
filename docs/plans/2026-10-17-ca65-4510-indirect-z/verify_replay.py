"""Verify the clean 4510 target replay and retained parser boundary."""

import argparse
import hashlib
import json
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("result", type=Path)
    args = parser.parse_args()
    report = json.loads((args.result / "report.json").read_text())
    assert (
        report["assembler"]["source_commit"]
        == "c4173c37c248190db833b4a7e873bd73d5d5b16d"
    )
    assert report["selected"] == ["cc65-4510-opcodes/opcode-space"]
    record = report["records"][0]
    assert record["source_commit"] == "e11fb5c39371046ebe25485f984f644c5a0d65d3"
    assert record["outcome"] == "assembler_rejected"
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
        assert process["exit"] == (1 if name == prefix + "assembler-0" else 0)
    artifact = record["reference_steps"][-1]["outputs"][0]
    data = bytes.fromhex(artifact["hex"])
    assert len(data) == artifact["length"] == 564
    assert (
        hashlib.sha256(data).hexdigest()
        == artifact["sha256"]
        == "8a5fcde630e55dd49403f7c6ffed25d8786e65e3d0a2723531b7ca3e2c0509ec"
    )
    assert (
        "4510-opcodes.s:18: error: expected `,X` or `,Y`"
        in processes[prefix + "assembler-0"]["stderr"]
    )
    print("Verified native 564-byte reference, next boundary and fourteen streams")


if __name__ == "__main__":
    main()
