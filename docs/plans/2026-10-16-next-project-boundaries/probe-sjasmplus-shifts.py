"""Capture SjASMPlus signed-word arithmetic, shifts and range refusals."""

import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path


def cases() -> dict[str, str]:
    expressions = {}
    lefts = ["0", "1", "2", "-1", "-2", "$7fffffff", "$80000000", "$ffffffff"]
    counts = [-65, -33, -32, -31, -1, 0, 1, 15, 31, 32, 33, 63, 64, 65]
    for operator in ["<<", ">>", "SHL", "SHR"]:
        for left in lefts:
            for count in counts:
                expressions[f"matrix_{operator}_{left}_{count}"] = (
                    f"({left}) {operator} ({count})"
                )
        for count in range(32):
            expressions[f"residue_{operator}_{count}"] = f"$80000001 {operator} {count}"
        for count in ["$80000000", "$ffffffff", "$7fffffff", "+32"]:
            expressions[f"wide_count_{operator}_{count}"] = f"1 {operator} ({count})"
    intermediate = [
        "(1<<31)>>31",
        "(1 SHL 31) SHR 31",
        "1 shl 32",
        "-1 shr 33",
        "$7fffffff+1",
        "($7fffffff+1)<0",
        "($ffffffff+1)==0",
        "$ffffffff<0",
        "($80000000/2)<0",
        "(($7fffffff+1)/2)>>1",
        "$7fffffff*$7fffffff",
        "($7fffffff*$7fffffff)<0",
        "(($7fffffff*$7fffffff)/2)>>1",
        "-(-2147483648)",
        "1<<(31+1)",
        "1<<($7fffffff+1)",
        "(1<<31)+1",
        "(1<<31)<0",
        "($7fffffff+1)>>31",
        "($7fffffff+1)/2",
        "(-2147483648)-1",
        "~$7fffffff",
        "(-(1<<31))<0",
        "1<<(1&&1)",
        "8>>(1||0)",
        "1<<(1==1)",
        "1<<((1<<31)+31)",
        "(($7fffffff+1)/2)==($80000000/2)",
        "$ffffffff|$80000000",
        "$ffffffff&$7fffffff",
        "$80000000^$ffffffff",
        "-(1<<31)-1",
    ]
    for index, expression in enumerate(intermediate):
        expressions[f"intermediate_{index}"] = expression
    literals = [
        "2147483647",
        "2147483648",
        "4294967295",
        "$7fffffff",
        "$80000000",
        "$ffffffff",
        "0xFFFFFFFF",
        "#FFFFFFFF",
        "0ffffffffh",
        "%" + "1" * 32,
        "0b" + "1" * 32,
        "1" * 32 + "b",
    ]
    for index, literal in enumerate(literals):
        expressions[f"literal_{index}"] = literal
    invalid = [
        "4294967296",
        "$100000000",
        "0x100000000",
        "#100000000",
        "100000000h",
        "%1" + "0" * 32,
        "0b1" + "0" * 32,
        "1" + "0" * 32 + "b",
        "1<<4294967296",
        "1<<(1/0)",
    ]
    for index, expression in enumerate(invalid):
        expressions[f"refusal_{index}"] = expression
    return {
        name: f"Value equ ({expression})\n dd ({expression}),Value\n IF ({expression})<0\n db $a5\n ELSE\n db $5a\n ENDIF\n ASSERT Value==({expression})\n"
        for name, expression in expressions.items()
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tool = Path(shutil.which("sjasmplus") or "/usr/local/bin/sjasmplus").absolute()
    records = []
    with tempfile.TemporaryDirectory(
        prefix="asm198x-next-boundaries-shifts-"
    ) as directory:
        root = Path(directory)

        def run(argv: list[str]) -> dict:
            result = subprocess.run(
                argv, cwd=root, capture_output=True, text=True, timeout=10, check=False
            )
            return {
                "argv": argv,
                "exit": result.returncode,
                "stdout": result.stdout,
                "stderr": result.stderr,
            }

        version = run([str(tool), "--version"])
        for name, source in cases().items():
            (root / "input.asm").write_text(source)
            (root / "native.bin").unlink(missing_ok=True)
            record = run([str(tool), "--raw=native.bin", "input.asm"])
            image = root / "native.bin"
            records.append(
                {
                    "name": name,
                    "source": source,
                    **record,
                    "hex": image.read_bytes().hex() if image.exists() else None,
                }
            )
    args.output.write_text(
        json.dumps(
            {
                "tool": str(tool),
                "sha256": hashlib.sha256(tool.read_bytes()).hexdigest(),
                "version": version,
                "cases": records,
            },
            indent=2,
        )
        + "\n"
    )
    print(
        len(records),
        "native cases;",
        sum(record["exit"] == 0 for record in records),
        "accepted",
    )
    for record in records:
        if record["exit"] != 0:
            print(record["name"], record["exit"], record["stderr"].splitlines()[-1])


if __name__ == "__main__":
    main()
