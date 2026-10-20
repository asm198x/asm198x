"""Measure each data element's native counter without changing source phases."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent
FILES: dict[str, str] = {}


def cases() -> dict[str, str]:
    bodies = {
        "byte_two": ".byte <*,<*\n",
        "byte_sum": ".byte <(*+*),<(*+*)\n",
        "byte_difference": ".byte *-*,*-*\n",
        "byte_ascii": '.byte "AB",<*,"C",<*\n',
        "byte_utf8": '.byte "é",<*\n',
        "byte_computed": '.byte .sprintf("%c",255),<*\n',
        "byte_partial": '.byte .sprintf("%.1s","é"),<*\n',
        "words_two": ".word *,*\n",
        "words_sum": ".word *+*,*+*\n",
        "words_difference": ".word *-*,*-*\n",
        "words_forward": ".word target-*,target-*\ntarget: .byte 7\n",
        "dbyt_two": ".dbyt *,*\n",
        "dbyt_difference": ".dbyt *-*,*-*\n",
        "dword_two": ".dword *,*\n",
        "dword_sum": ".dword *+*,*+*\n",
        "faraddr_two": ".faraddr *,*\n",
        "faraddr_difference": ".faraddr *-*,*-*\n",
        "addr_two": ".addr *,*\n",
        "addr_sum": ".addr *+*,*+*\n",
        "lobytes": ".lobytes *,*\n",
        "hibytes": ".hibytes *+$ff,*+$ff\n",
        "bankbytes": ".bankbytes *+$efff,*+$efff\n",
        "high_nested": ".byte >(*+$ff),>(*+$ff)\n",
        "bank_nested": ".byte ^(*+$efff),^(*+$efff)\n",
        "byte_overflow": ".byte *-$f01,*-$f01\n",
        "word_overflow": ".word *+$efff,*+$efff\n",
        "dbyt_overflow": ".dbyt *+$efff,*+$efff\n",
        "dword_overflow": ".dword *+$ffffefff,*+$ffffefff\n",
        "faraddr_overflow": ".faraddr *+$ffeffd,*+$ffeffd\n",
        "addr_wrap": ".addr *+$efff,*+$efff\n",
        "separate_lines": ".word *\n.word *\n",
        "literal_asterisk": '.byte "*",<*,"**",<*\n',
        "multiplication": ".word * * 2,* * 2\n",
        "negation": ".word -*+$2000,-*+$2000\n",
        "complement": ".word ~* & $ffff,~* & $ffff\n",
        "logical": ".byte !(* & 1),!(* & 1)\n",
        "named_constant": "COUNT=2\n.word (*+COUNT),(*+COUNT)\n",
        "mixed_width": ".byte <*\n.word *,*\n.faraddr *,*\n",
    }
    assert len(bodies) == 38
    out = {}
    for cpu in ["65816", "huc6280"]:
        preamble = f'.setcpu "{cpu}"\n.org $1000\n'
        for kind, body in bodies.items():
            for context, source in {
                "direct": body,
                "macro": ".macro emit\n" + body + ".endmacro\nemit\n",
                "included": body,
                "unused": ".macro unused\n" + body + ".endmacro\n.byte 7\n",
            }.items():
                name = f"{cpu}_{kind}_{context}"
                if context == "included":
                    filename = name + ".inc"
                    FILES[filename] = source
                    source = f'.include "{filename}"\n'
                out[name] = preamble + source
    assert len(out) == 304
    return out


if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location(
        "capture", ROOT.parent / "2026-10-19-ca65-scope-binding/probe.py"
    )
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    prepared = cases()
    runner.FILES = FILES
    runner.cases = lambda: prepared
    runner.main()
