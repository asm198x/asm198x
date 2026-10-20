"""Measure complete native byte-string construction and precision boundaries."""

import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}

    def data(expr: str, directive: str = ".byte") -> str:
        return ".byte 6\n" + directive + " " + expr + "\n.byte 7\n"

    for value in [*range(256), -1, 256, 257, 65535, 65536, 4294967295]:
        out[f"character_{value}"] = data(f'.sprintf("%c",{value})')
    for value in [0, 10, 34, 92, 127, 128, 255]:
        out[f"asciiz_{value}"] = data(f'.sprintf("%c",{value})', ".asciiz")
    for value in [0, 34, 92, 128, 255]:
        out[f"concat_character_{value}"] = data(f'.concat(.sprintf("%c",{value}),"X")')
        out[f"string_character_{value}"] = data(
            f'.sprintf("%s",.sprintf("%c",{value}))'
        )
    for fmt in ["%s", "%.1s", "%3s", "%3.1s"]:
        out[f"utf8_{fmt}"] = data(f'.sprintf("{fmt}","é")')
    out["raw_utf8"] = data('"é"')
    out["word_string"] = data('.sprintf("%c",65)', ".word")
    out["dword_string"] = data('.sprintf("%c",65)', ".dword")
    out["immediate_string"] = '.byte 6\nlda #.sprintf("%c",65)\n.byte 7\n'
    return out


if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location(
        "capture", Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
    )
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
