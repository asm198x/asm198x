"""Capture SjASMPlus #635 vocabulary and composition facts with bounded runs."""

import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

TOOL = Path("/usr/local/bin/sjasmplus").resolve()
EXPRESSIONS = [
    "1 XOR 2",
    "1 xor 2",
    "1 xOr 2",
    "1 AND 2",
    "1 and 2",
    "1 aNd 2",
    "1 OR 2",
    "1 or 2",
    "1 oR 2",
    "1 SHL 2",
    "1 shl 2",
    "1 sHl 2",
    "8 SHR 1",
    "8 shr 1",
    "8 sHr 1",
    "NOT 1",
    "NOT 0",
    "NOT 2",
    "not 0",
    "nOt 0",
    "NOT NOT 0",
    "NOT -1",
    "NOT 1+1",
    "NOT 0+1",
    "NOT 1 AND 0",
    "1 OR 2 XOR 3",
    "1 XOR 2 OR 3",
    "1 OR 2 AND 4",
    "1 XOR 2 AND 4",
    "1 SHL 2+1",
    "2*1 SHL 2",
    "-1 SHR 1",
]
CASES = {
    f"expr_{i}": " org 0\n dw " + expr + "\n" for i, expr in enumerate(EXPRESSIONS)
}
CASES.update(
    {
        "equ": "V equ NOT 0\n dw V,V AND 2\n",
        "condition": " if NOT 0 AND 2\n db 7\n else\n db 9\n endif\n",
        "precedence_condition": " if 1 OR 2 XOR 3\n db 7\n else\n db 9\n endif\n",
        "boundaries": "ANDY equ 4\nxor_value equ 5\nshrink equ 6\n dw ANDY,xor_value,shrink\n",
        "adjacent_boundary": " dw 1 ANDY 2\n",
        "macro_repeat": "put macro x\n dw x XOR 2\n endm\n rept 2\n put 1\n endr\n",
    }
)


def main() -> None:
    rows = []
    version = subprocess.run(
        [str(TOOL), "--version"],
        capture_output=True,
        text=True,
        timeout=10,
        check=False,
    )
    with tempfile.TemporaryDirectory(prefix="asm198x-sj-words-") as directory:
        root = Path(directory)
        for name, source in CASES.items():
            (root / "input.asm").write_text(source)
            image = root / "out.bin"
            image.unlink(missing_ok=True)
            argv = [str(TOOL), "--raw=out.bin", "input.asm"]
            result = subprocess.run(
                argv, cwd=root, capture_output=True, text=True, timeout=10, check=False
            )
            rows.append(
                {
                    "name": name,
                    "source": source,
                    "argv": argv,
                    "exit": result.returncode,
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                    "hex": image.read_bytes().hex() if image.exists() else None,
                }
            )
            print(name, result.returncode, rows[-1]["hex"])
    Path("crates/asm198x/tests/fixtures/sjasmplus-word-operators.json").write_text(
        json.dumps(
            {
                "tool": str(TOOL),
                "sha256": hashlib.sha256(TOOL.read_bytes()).hexdigest(),
                "version": version.stdout + version.stderr,
                "cases": rows,
            },
            indent=2,
        )
        + "\n"
    )


if __name__ == "__main__":
    main()
