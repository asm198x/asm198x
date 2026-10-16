"""Capture native SjASMPlus expression truth, precedence and eager evaluation."""

import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

EXPRESSIONS = [
    "(1==1)&&(2==2)",
    "1&&2",
    "0||2",
    "!0",
    "!2",
    "!!1",
    "1==1",
    "1!=2",
    "1<2",
    "1<=2",
    "1>2",
    "1>=2",
    "1|2==3",
    "1&2==0",
    "1^3==2",
    "1==1==1",
    "1==2<3",
    "1<2==0",
    "2==3>0",
    "1==1!=0",
    "1==1<0",
    "1|0==0",
    "2&3==3",
    "2 AND 3==3",
    "0||2&3",
    "(0||2)==-1",
    "(1&&2)==-1",
    "!0&2",
    "NOT 0&2",
    "!(1|2)",
    "NOT 1+1",
    "0&&(1/0)",
    "1||(1/0)",
    "0&&undefined",
    "1||undefined",
    "(1==1)&&3==3",
    "1||0&&0",
    "2+3==5&&4",
    "!1==0",
    "1+2<<1==6&&3*4==12",
    "1&&",
    "||2",
    "!",
    "1<>2",
    "1=1",
    "1===1",
]


def cases() -> dict[str, str]:
    result = {}
    for i, expression in enumerate(EXPRESSIONS):
        result[f"data_{i}"] = f" dw {expression}\n"
        result[f"equ_{i}"] = f"V equ {expression}\n dw V\n"
        result[f"if_{i}"] = f" IF {expression}\n db $11\n ELSE\n db $22\n ENDIF\n"
        result[f"assert_{i}"] = f" ASSERT {expression}\n db $33\n"
    result.update(
        {
            "struct_assert": " STRUCT Channel\nmember WORD 0\nflag BYTE 0\n ENDS\n ASSERT (Channel==3)&&Channel.member==0\n dw Channel,Channel.flag\n",
            "forward_assert": " ASSERT (future==5)&&1\nfuture equ 5\n dw future\n",
            "forward_if": " IF (future==5)&&1\n db $11\n ELSE\n db $22\n ENDIF\nfuture equ 5\n",
            "macro_repeat": "put macro x\n ASSERT (x==2)&&1\n dw (x==2)||0\n endm\n rept 2\n put 2\n endr\n",
            "value_if": "V equ 1&&2\n IF V==-1\n db $11\n ELSE\n db $22\n ENDIF\n",
            "trailing_if_text": " IF 1&&2 ignored\n db $11\n ENDIF\n",
        }
    )
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tool = Path(shutil.which("sjasmplus") or "/usr/local/bin/sjasmplus").absolute()
    records = []
    with tempfile.TemporaryDirectory(
        prefix="asm198x-next-boundaries-sj-logic-"
    ) as directory:
        root = Path(directory)

        def run(argv: list[str]) -> dict:
            p = subprocess.run(
                argv, cwd=root, capture_output=True, text=True, timeout=10, check=False
            )
            return {
                "argv": argv,
                "exit": p.returncode,
                "stdout": p.stdout,
                "stderr": p.stderr,
            }

        version = run([str(tool), "--version"])
        for name, source in cases().items():
            (root / "input.asm").write_text(source)
            (root / "native.bin").unlink(missing_ok=True)
            r = run([str(tool), "--raw=native.bin", "input.asm"])
            image = root / "native.bin"
            records.append(
                {
                    "name": name,
                    "source": source,
                    **r,
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
        len(records), "native cases;", sum(r["exit"] == 0 for r in records), "accepted"
    )
    for r in records[-6:]:
        print(r["name"], r["exit"], r["hex"])


if __name__ == "__main__":
    main()
