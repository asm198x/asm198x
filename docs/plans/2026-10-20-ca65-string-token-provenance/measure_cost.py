"""Measure paired source-prepass cost against independently assembled images."""

import hashlib
import json
import os
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

assert len(sys.argv) == 4, "usage: measure_cost.py PREDECESSOR BINARY OUTPUT"
out = Path(sys.argv[3])
binaries = {
    "predecessor": Path(sys.argv[1]).resolve(strict=True),
    "byte_store": Path(sys.argv[2]).resolve(strict=True),
}
sources = {
    "repeat": '.repeat 10000\n.byte .sprintf("%c",65)\n.endrepeat\n',
    "macro_repeat": '.macro emit\n.byte .sprintf("%c",65)\n.endmacro\n.repeat 10000\nemit\n.endrepeat\n',
    "ordinary_repeat": ".repeat 10000\nnop\n.endrepeat\n",
}
facts = {
    "host": sys.platform,
    "rss_unit": "bytes",
    "binaries": {
        name: {
            "path": str(path),
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        }
        for name, path in binaries.items()
    },
    "cases": [],
}
with tempfile.TemporaryDirectory(prefix="asm198x-byte-string-cost-") as temp:
    root = Path(temp)
    (root / "input.cfg").write_text(
        "MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { CODE:load=MAIN,type=ro; }\n"
    )

    def run(argv: list[str]) -> dict[str, Any]:
        started = time.perf_counter()
        with (
            (root / "process.stdout").open("wb") as stdout,
            (root / "process.stderr").open("wb") as stderr,
        ):
            p = subprocess.Popen(
                argv, cwd=root, stdout=stdout, stderr=stderr, start_new_session=True
            )
            try:
                while True:
                    pid, status, usage = os.wait4(p.pid, os.WNOHANG)
                    if pid:
                        p.returncode = os.waitstatus_to_exitcode(status)
                        break
                    if time.perf_counter() - started > 120:
                        raise TimeoutError(argv)
                    time.sleep(0.01)
            except BaseException:
                os.killpg(p.pid, signal.SIGKILL)
                os.wait4(p.pid, 0)
                p.returncode = -signal.SIGKILL
                raise
        stdout = (root / "process.stdout").read_text()
        stderr = (root / "process.stderr").read_text()
        assert p.returncode == 0, (argv, p.returncode, stderr)
        return {
            "argv": argv,
            "exit": p.returncode,
            "elapsed_seconds": time.perf_counter() - started,
            "stdout": stdout,
            "stderr": stderr,
            "max_rss_bytes": usage.ru_maxrss
            * (1 if sys.platform == "darwin" else 1024),
            "user_seconds": usage.ru_utime,
            "system_seconds": usage.ru_stime,
        }

    for name, source in sources.items():
        (root / "input.s").write_text(source)
        native = [
            run(["/opt/homebrew/bin/ca65", "input.s", "-o", "input.o"]),
            run(
                [
                    "/opt/homebrew/bin/ld65",
                    "-C",
                    "input.cfg",
                    "input.o",
                    "-o",
                    "native.bin",
                ]
            ),
        ]
        expected = (root / "native.bin").read_bytes()
        assert len(expected) == 10000
        record = {
            "name": name,
            "source": source,
            "config": (root / "input.cfg").read_text(),
            "native": native,
            "native_length": len(expected),
            "native_sha256": hashlib.sha256(expected).hexdigest(),
            "runs": [],
        }
        for repeat in range(3):
            for binary, path in binaries.items():
                (root / "out.bin").unlink(missing_ok=True)
                row = run(
                    [
                        str(path),
                        "--dialect",
                        "ca65",
                        "-C",
                        "input.cfg",
                        "input.s",
                        "-o",
                        "out.bin",
                    ]
                )
                assert (root / "out.bin").read_bytes() == expected
                row["binary"] = binary
                row["repeat"] = repeat
                assert row["max_rss_bytes"] > 0
                record["runs"].append(row)
        facts["cases"].append(record)
        out.write_text(json.dumps(facts, indent=2) + "\n")
        print(name, "complete", flush=True)
