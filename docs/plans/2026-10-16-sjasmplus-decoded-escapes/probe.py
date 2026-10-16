"""Capture #696 composition and malformed-input cases with native SjASMPlus."""

import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

TOOL = Path("/usr/local/bin/sjasmplus").resolve()
CASES = {
    "escaped_quote_delimiters": r' db "a\";,//b",7 ; real' + "\n",
    "escaped_backslash_end": r' db "a\\",7' + "\n",
    "unknown_then_quote": r' db "a\q\";b",7' + "\n",
    "slash_comma_quotes": r""" db "a\";b",',',';',"c\n//d",7 // real""" + "\n",
    "literal_single_backslash": r""" db '\',7""" + "\n",
    "marked": r' dc "a\n","\0",7,"b\""' + "\n",
    "terminated": r' dz "a\n",7,"b\\"' + "\n",
    "offset_marked": r' abytec 1 "a\n",7,"b\""' + "\n",
    "offset_terminated": r' abytez 1 "a\n",7' + "\n",
    "word": r' dw "a\n",7' + "\n",
    "dword": r' dd "a\n",7' + "\n",
    "triple": r' d24 "a\n",7' + "\n",
    "empty": r' db "",7' + "\n",
    "macro_repeat": "put macro\n"
    + r' db "a\n;,//b",7'
    + "\n endm\n rept 2\n put\n endr\n",
    "include": ' include "data.inc"\n',
    "missing_quote": r' db "a\";b,7' + "\n",
    "dangling_backslash": ' db "a' + chr(92) + "\n",
    "trailing_tokens": r' db "a\n"oops,7' + "\n",
    "missing_operand": r' db "a\n",?' + "\n",
    "trailing_comma": r' db "a\n",' + "\n",
}
for directive in ["db", "dw", "dd", "d24"]:
    for text in ["", "a", "ab", "abc", "abcd", "abcde", "abcdef", "é", "a\\n"]:
        CASES[f"packing_{directive}_{text}"] = f' {directive} "{text}",7\n'

INCLUDE = r' db "a\";b\n//c",7 ; real' + "\n"


def main() -> None:
    rows = []
    version = subprocess.run(
        [str(TOOL), "--version"],
        capture_output=True,
        text=True,
        timeout=10,
        check=False,
    )
    with tempfile.TemporaryDirectory(prefix="asm198x-sj-escapes-") as directory:
        root = Path(directory)
        (root / "data.inc").write_text(INCLUDE)
        for name, body in CASES.items():
            source = " org $8000\n" + body
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
                    "files": {"data.inc": INCLUDE} if name == "include" else {},
                    "argv": argv,
                    "exit": result.returncode,
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                    "hex": image.read_bytes().hex() if image.exists() else None,
                }
            )
            print(name, result.returncode, rows[-1]["hex"])
    Path("crates/asm198x/tests/fixtures/sjasmplus-decoded-composition.json").write_text(
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
