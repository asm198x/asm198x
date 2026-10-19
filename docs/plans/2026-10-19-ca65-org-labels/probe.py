"""Compare a label on a mode directive with the same label just before it."""

import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    for mode in ["org", "reorg", "reloc"]:
        for offset in [0, 1, 3]:
            for namespace in ["global", "cheap", "scope", "procedure", "anonymous"]:
                for inline in [False, True]:
                    prefix = "" if mode == "org" else ".org $2000\n"
                    prefix += f".res {offset},$55\n"
                    label = (
                        "@item"
                        if namespace == "cheap"
                        else ""
                        if namespace == "anonymous"
                        else "item"
                    )
                    reference = ":-" if namespace == "anonymous" else label
                    directive = (
                        ".reloc"
                        if mode == "reloc"
                        else ".org $3000"
                        if mode == "reorg"
                        else ".org $2000"
                    )
                    body = prefix + ("head:\n" if namespace == "cheap" else "")
                    body += label + ":" + ("" if inline else "\n") + directive + "\n"
                    body += f".word {reference},*\nALIAS={reference}\n.word ALIAS\n"
                    if namespace == "scope":
                        body = ".scope inner\n" + body + ".endscope\n"
                    elif namespace == "procedure":
                        body = ".proc outer\n" + body + ".endproc\n"
                    out[f"{mode}_{offset}_{namespace}_{inline}"] = body
    for inline in [False, True]:
        declaration = "item:" + ("" if inline else "\n") + ".org $2000\n.word item,*\n"
        out[f"macro_{inline}"] = ".macro shift\n" + declaration + ".endmacro\nshift\n"
    out.update(
        {
            "include": '.include "data.inc"\n',
            "repeat": '.repeat 2,I\n.ident(.sprintf("P%d",I)):.org $2000+I\n.word .ident(.sprintf("P%d",I)),*\n.endrepeat\n',
            "zp": '.segment "ZEROPAGE"\n.org $30\nitem:.reloc\n.segment "CODE"\n.word item\n',
            "no_mode_label": "item:.byte 7\n.word item\n",
            "reloc_already_relative": ".byte 7\nitem:.reloc\n.word item\n",
            "zero_org": "item:.org 0\n.word item,*\n",
            "explicit_absolute": "item:.org $2000\nlda a:item\n",
            "duplicate": ".org $2000\nitem:.byte 7\nitem:.reloc\n.word item\n",
            "range_error": "item:.org $2000\n.word 65536\n",
        }
    )
    return out


if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location(
        "capture", Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
    )
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.CONFIG = 'MEMORY { MAIN:start=$1000,size=$7000,file=%O; RAM:start=$20,size=$100,file=""; }\nSEGMENTS { CODE:load=MAIN,type=ro; ZEROPAGE:load=RAM,type=zp; }\n'
    runner.FILES = {"data.inc": "item:.org $2000\n.word item,*\n"}
    runner.cases = cases
    runner.main()
