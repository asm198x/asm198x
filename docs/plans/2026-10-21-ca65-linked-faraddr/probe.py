"""Capture configured ca65 far-address range boundaries and extraction controls."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent
FILES: dict[str, str] = {}
BODIES: dict[str, str] = {
    "minimum": ".faraddr 0\n",
    "maximum": ".faraddr $ffffff\n",
    "negative_one": ".faraddr -1\n",
    "negative_large": ".faraddr -$800000\n",
    "overflow": ".faraddr $1000000\n",
    "overflow_large": ".faraddr $ffffffff\n",
    "backward_negative": "HERE:.byte 7\n.faraddr HERE-HERE-1\n",
    "backward_overflow": "HERE:.byte 7\n.faraddr HERE-HERE+$1000000\n",
    "forward_label_negative": ".faraddr TARGET-TARGET-1\nTARGET:.byte 7\n",
    "forward_label_overflow": ".faraddr TARGET-TARGET+$1000000\nTARGET:.byte 7\n",
    "forward_alias_negative": ".faraddr TARGET\nTARGET=-1\n",
    "forward_alias_overflow": ".faraddr TARGET\nTARGET=$1000000\n",
    "query_negative": "VALUE=2\n.faraddr .const(VALUE)-2\n",
    "query_zero": "VALUE=2\n.faraddr .const(VALUE)-1\n",
    "extraction_negative": ".lobytes -1\n.hibytes -1\n.bankbytes -1\n",
    "word_negative": ".word -1\n",
    "byte_negative": ".byte -1\n",
    "label_difference_zero": "HERE:.byte 7\n.faraddr HERE-HERE\n",
}


def cases() -> dict[str, str]:
    out: dict[str, str] = {}
    for name, body in BODIES.items():
        for context in ["direct", "included", "macro", "inactive", "unused"]:
            key = name + "_" + context
            if context == "included":
                FILES[key + ".inc"] = body
                text = f'.include "{key}.inc"\n'
            elif context in ["macro", "unused"]:
                text = (
                    ".macro run\n"
                    + body
                    + ".endmacro\n"
                    + ("run\n" if context == "macro" else ".byte 9\n")
                )
            elif context == "inactive":
                text = ".if 0\n" + body + ".endif\n.byte 9\n"
            else:
                text = body
            out[key] = text
    assert len(out) == 90
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
