"""Capture SAVENEX output without executing upstream scripts."""

import argparse
import gzip
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path


def cases() -> dict[str, str]:
    device = " DEVICE ZXSPECTRUMNEXT\n"
    open_ = ' SAVENEX OPEN "out.nex",$8000,$b000\n'
    out = {
        "original_shape": device
        + " org $8000\n db 1,2,3\n"
        + open_
        + " SAVENEX CORE 3,0,0\n SAVENEX AUTO\n SAVENEX CLOSE\n",
        "default_open": device + ' SAVENEX OPEN "out.nex"\n SAVENEX CLOSE\n',
        "implicit_close": device + open_,
        "open_twice": device + open_ + open_,
        "no_device": open_ + " SAVENEX CLOSE\n",
        "wrong_device": " DEVICE ZXSPECTRUM128\n" + open_ + " SAVENEX CLOSE\n",
        "empty_auto": device + open_ + " SAVENEX AUTO\n SAVENEX CLOSE\n",
        "repeat_auto": device
        + open_
        + " SAVENEX AUTO\n SAVENEX AUTO\n SAVENEX CLOSE\n",
        "core_before_open": device + " SAVENEX CORE 3,0,0\n",
        "close_without_open": device + " SAVENEX CLOSE\n",
        "auto_without_open": device + " SAVENEX AUTO\n",
        "two_outputs": device
        + open_
        + ' SAVENEX CLOSE\n SAVENEX OPEN "other.nex",$9000\n SAVENEX CLOSE\n',
        "inactive": " IF 0\n SAVENEX BOGUS\n ENDIF\n db 1\n",
        "snapshot": device
        + " org $8000\n db 1\n"
        + open_
        + " SAVENEX AUTO\n org $8000\n db 2\n SAVENEX CLOSE\n",
        "forward": device
        + ' SAVENEX OPEN "out.nex",entry,stack\n SAVENEX CLOSE\nentry equ $8123\nstack equ $b456\n',
    }
    for index, args in enumerate(
        [
            "",
            "0",
            "$ffff,0",
            "-1,65536",
            "$8000,$b000,111",
            "$8000,$b000,112",
            "$8000,$b000,-1",
            "$8000,$b000,0,2",
            "$8000,$b000,0,3",
            "$8000,$b000,0,1",
            "$8000,$b000,0,4",
            "$8000,",
            "$8000,,2",
        ]
    ):
        out[f"open_{index}"] = (
            device
            + ' SAVENEX OPEN "out.nex"'
            + ("," + args if args else "")
            + "\n SAVENEX CLOSE\n"
        )
    for index, args in enumerate(
        ["3,0,0", "15,15,255", "16,16,256", "-1,-1,-1", "1,2", "1,2,3,4"]
    ):
        out[f"core_{index}"] = (
            device + open_ + f" SAVENEX CORE {args}\n SAVENEX CLOSE\n"
        )
    for index, args in enumerate(
        ["", "0", "5,2", "2,5", "111,111", "112", "-1", "0,112", "0,1,2"]
    ):
        out[f"auto_{index}"] = (
            device
            + " org $8000\n db $a5\n"
            + open_
            + f" SAVENEX AUTO {args}\n SAVENEX CLOSE\n"
        )
    for bank in [0, 1, 2, 3, 4, 5, 6, 47, 48, 111]:
        out[f"bank_{bank}"] = (
            device
            + f" SLOT 6\n PAGE {bank * 2}\n org $c000\n db $a5\n"
            + open_
            + " SAVENEX AUTO\n SAVENEX CLOSE\n"
        )
    return out


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    images = args.output / "images"
    images.mkdir()
    tool = Path("/usr/local/bin/sjasmplus")
    records = []
    with tempfile.TemporaryDirectory(prefix="asm198x-savenex-probe-") as temp:
        root = Path(temp)

        def run(argv: list[str]) -> dict:
            p = subprocess.run(
                argv, cwd=root, capture_output=True, timeout=10, check=False
            )
            return {
                "argv": argv,
                "exit": p.returncode,
                "stdout": p.stdout.decode(errors="replace"),
                "stderr": p.stderr.decode(errors="replace"),
            }

        version = run([str(tool), "--version"])
        for name, source in cases().items():
            for file in ["out.nex", "other.nex", "raw.bin"]:
                (root / file).unlink(missing_ok=True)
            (root / "input.asm").write_text(source)
            result = run(
                [
                    str(tool),
                    "--nologo",
                    "--color=off",
                    "--msg=war",
                    "--raw=raw.bin",
                    "input.asm",
                ]
            )
            outputs = []
            for file in ["out.nex", "other.nex", "raw.bin"]:
                p = root / file
                if not p.exists():
                    continue
                data = p.read_bytes()
                digest = hashlib.sha256(data).hexdigest()
                saved = images / (digest + ".gz")
                if not saved.exists():
                    saved.write_bytes(gzip.compress(data, mtime=0))
                outputs.append(
                    {
                        "name": file,
                        "length": len(data),
                        "sha256": digest,
                        "image": "images/" + saved.name,
                        "header_hex": data[:512].hex()
                        if file.endswith(".nex")
                        else None,
                    }
                )
            records.append(
                {"name": name, "source": source, **result, "outputs": outputs}
            )
    (args.output / "report.json").write_text(
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
    for r in records:
        print(
            r["name"],
            r["exit"],
            [
                (
                    o["name"],
                    o["length"],
                    o["header_hex"][18:20] if o["header_hex"] else "",
                )
                for o in r["outputs"]
            ],
            r["stderr"].strip(),
        )


if __name__ == "__main__":
    main()
