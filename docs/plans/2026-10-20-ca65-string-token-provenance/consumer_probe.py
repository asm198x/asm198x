"""Native contracts at computed ca65 string consumer boundaries."""

import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    values = {
        "ascii": '"A"',
        "slash": r'"a\b"',
        "two_slashes": r'"a\\b"',
        "utf8": '"é"',
        "quote": '.sprintf("%c",34)',
        "high": '.sprintf("%c",255)',
        "partial_utf8": '.sprintf("%.1s","é")',
    }
    for label, value in values.items():
        for name, expr in {
            "length": f".strlen({value})",
            "first": f".strat({value},0)",
            "last": f".strat({value},.strlen({value})-1)",
            "past": f".strat({value},.strlen({value}))",
            "concat_length": f'.strlen(.concat({value},"X"))',
            "token_count": f".tcount({{{value}}})",
            "token_equal": f".xmatch({{{value}}},{{{value}}})",
            "token_select": f".left(1,{{{value}}})",
        }.items():
            out[f"{label}_{name}"] = f".byte 6\n.byte {expr}\n.byte 7\n"
    for label, expr in {
        "cpu": '.setcpu .concat("65","02")\n.byte 7\n',
        "segment": '.segment .concat("CO","DE")\n.byte 7\n',
        "include": '.include .concat("p","art.s")\n.byte 7\n',
        "incbin": '.incbin .concat("p","art.bin")\n.byte 7\n',
        "identifier": 'NAME=9\n.byte .ident(.concat("NA","ME"))\n',
        "message": '.warning .concat("a","b")\n.byte 7\n',
        "assert": '.assert 1,error,.concat("a","b")\n.byte 7\n',
        "numeric_format": '.byte .sprintf("%d",.concat("1","2"))\n',
        "numeric_selector": '.byte .left(.concat("1","2"),{1})\n',
    }.items():
        out[label] = expr
    return out


if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location(
        "capture", Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
    )
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.FILES = {"part.s": ".byte 8\n", "part.bin": "X"}
    runner.main()
