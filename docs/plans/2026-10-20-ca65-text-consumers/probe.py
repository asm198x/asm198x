"""Capture native UTF-8 text consumer payloads with raw process streams."""

import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

CONFIG = "MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { CODE:load=MAIN,type=ro; }\n"
VALUES = {
    "quote": ('.sprintf("%c",34)', '"'),
    "slash": ('.sprintf("%c",92)', "\\"),
    "comma": ('.sprintf("%c",44)', ","),
    "semicolon": ('.sprintf("%c",59)', ";"),
    "apostrophe": ('.sprintf("%c",39)', "'"),
    "space": ('.sprintf("%c",32)', " "),
    "tab": ('.sprintf("%c",9)', "\t"),
    "newline": ('.sprintf("%c",10)', "\n"),
    "utf8": ('.concat("é")', "é"),
    "empty": ('.concat("")', ""),
    "mixed_quote": ('.concat("a",.sprintf("%c",34),"b")', 'a"b'),
}


def main() -> None:
    tools = {name: Path("/opt/homebrew/bin") / name for name in ["ca65", "ld65"]}
    files = {}
    cases = []
    with tempfile.TemporaryDirectory(prefix="asm198x-native-text-") as td:
        root = Path(td)

        def run(argv: list[str]) -> dict:
            p = subprocess.run(
                argv, cwd=root, capture_output=True, timeout=10, check=False
            )
            return {
                "argv": argv,
                "cwd": str(root),
                "exit": p.returncode,
                "stdout": p.stdout.decode(errors="replace"),
                "stderr": p.stderr.decode(errors="replace"),
                "stdout_hex": p.stdout.hex(),
                "stderr_hex": p.stderr.hex(),
            }

        identities = {
            name: {
                "path": str(path),
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "version": run([str(path), "--version"]),
            }
            for name, path in tools.items()
        }
        for index, (name, (expression, value)) in enumerate(VALUES.items()):
            files["inc" + value] = f".byte {20 + index}\n"
            files["bin" + value] = chr(80 + index)
        for name, text in files.items():
            (root / name).write_text(text)
        (root / "input.cfg").write_text(CONFIG)
        for name, (expression, value) in VALUES.items():
            for consumer, body in {
                "include": f'.include .concat("inc",{expression})\n',
                "incbin": f'.incbin .concat("bin",{expression})\n',
                "warning": f".warning {expression}\n",
                "error": f".error {expression}\n",
                "fatal": f".fatal {expression}\n",
                "assert_error": f".assert 0,error,{expression}\n",
                "assert_warning": f".assert 0,warning,{expression}\n",
                "assert_true": f".assert 1,error,{expression}\n",
                "out": f".out {expression}\n",
            }.items():
                for context, contents in {
                    "direct": body,
                    "labelled": "here: " + body,
                    "selected": ".if 1\n" + body + ".endif\n",
                    "macro_body": ".macro emit\n" + body + ".endmacro\nemit\n",
                }.items():
                    source = ".byte 6\n" + contents + ".byte 7\n"
                    (root / "input.s").write_text(source)
                    for output in ["input.o", "native.bin"]:
                        (root / output).unlink(missing_ok=True)
                    native = [run([str(tools["ca65"]), "input.s", "-o", "input.o"])]
                    if native[-1]["exit"] == 0:
                        native.append(
                            run(
                                [
                                    str(tools["ld65"]),
                                    "-C",
                                    "input.cfg",
                                    "input.o",
                                    "-o",
                                    "native.bin",
                                ]
                            )
                        )
                    cases.append(
                        {
                            "name": f"{context}_{consumer}_{name}",
                            "consumer": consumer,
                            "value": value,
                            "source": source,
                            "config": CONFIG,
                            "native": native,
                            "native_hex": (root / "native.bin").read_bytes().hex()
                            if (root / "native.bin").exists()
                            else None,
                        }
                    )
        report = {"tools": identities, "files": files, "cases": cases}
    Path(__file__).with_name("native.json").write_text(
        json.dumps(report, indent=2) + "\n"
    )
    assert len(cases) == 396
    print(
        "cases",
        len(cases),
        "accepted",
        sum(c["native"][-1]["exit"] == 0 for c in cases),
    )


if __name__ == "__main__":
    main()
