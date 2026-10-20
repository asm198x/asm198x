"""Native CPU-specific address extraction and strict word controls."""

import importlib.util
from pathlib import Path
from types import ModuleType

ROOT = Path(__file__).parent
FILES: dict[str, str] = {}


def load(path: Path) -> ModuleType:
    spec = importlib.util.spec_from_file_location("capture", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def cases() -> dict[str, str]:
    out = {}
    values = [
        "-65537",
        "-65536",
        "-32769",
        "-32768",
        "-1",
        "0",
        "1",
        "32767",
        "65535",
        "65536",
        "16777215",
        "16777216",
        "$FFFFFFFF",
        "4294967295+1",
        "-$FFFFFFFF-1",
        "<$10000",
        ">$FFFFFF",
    ]
    for cpu in ["65816", "huc6280"]:
        preamble = f'.setcpu "{cpu}"\n.org $1000\n'
        for index, value in enumerate(values):
            for context, body in {
                "direct": f".addr {value}\n",
                "macro": f".macro emit value\n.addr value\n.endmacro\nemit {value}\n",
                "included": f".addr {value}\n",
                "unused": f".macro unused\n.addr {value}\n.endmacro\n.byte 7\n",
            }.items():
                name = f"{cpu}_{index}_{context}"
                if context == "included":
                    filename = name + ".inc"
                    FILES[filename] = body
                    body = f'.include "{filename}"\n'
                out[name] = preamble + body
        for context, body in {
            "forward_bank": ".addr target+$10000\ntarget: .byte 7\n",
            "forward_negative": ".addr target-$2000\ntarget: .byte 7\n",
            "two_values": ".addr 1,$1ffff\n",
            "word_control": ".word 65536\n",
            "uppercase": ".ADDR $1ffff\n",
            "missing": ".addr MISSING\n",
        }.items():
            out[f"{cpu}_{context}"] = preamble + body
    assert len(out) == 148
    return out


if __name__ == "__main__":
    runner = load(ROOT.parent / "2026-10-19-ca65-scope-binding/probe.py")
    prepared = cases()
    runner.FILES = FILES
    runner.cases = lambda: prepared
    runner.main()
