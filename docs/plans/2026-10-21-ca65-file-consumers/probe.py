"""Measure native ordered include/incbin operand consumption for #795."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
FILES = {
    "payload.bin": "ABCD",
    **{f"payload{x}.bin": "ABCD" for x in range(4)},
    **{f"selected{x}.inc": f".byte {x},.ref(VALUE)\n" for x in range(4)},
}
BODIES = {
    "include_value": '.include .sprintf("selected%d.inc",VALUE)',
    "include_order": '.include .sprintf("selected%d.inc",.ref(VALUE)+.const(VALUE))',
    "include_reverse": '.include .sprintf("selected%d.inc",.const(VALUE)+.ref(VALUE))',
    "include_nested": '.include .concat("selected",.sprintf("%d",VALUE),".inc")',
    "include_selector": '.include .left(50,{.sprintf("selected%d.inc",VALUE)})',
    "include_missing": '.include .sprintf("selected%d.inc",MISSING)',
    "include_query_only": '.include .sprintf("selected%d.inc",.const(.ref(MISSING)))',
    "incbin_offset": '.incbin "payload.bin",.const(VALUE),.ref(VALUE)',
    "incbin_order": '.incbin "payload.bin",.ref(VALUE)+.const(VALUE),.ref(VALUE)',
    "incbin_reverse": '.incbin "payload.bin",.const(VALUE)+.ref(VALUE),.ref(VALUE)',
    "incbin_size": '.incbin "payload.bin",0,.const(VALUE)+.ref(VALUE)',
    "incbin_zero_size": '.incbin "payload.bin",.const(VALUE),0',
    "incbin_filename": '.incbin .sprintf("payload%d.bin",VALUE),.ref(VALUE),.const(VALUE)',
    "incbin_query_filename": '.incbin .sprintf("payload%d.bin",.ref(VALUE)),.const(VALUE),.ref(VALUE)',
    "incbin_nested_offset": '.incbin "payload.bin",.strat(.sprintf("%d",VALUE),0)-48,.ref(VALUE)',
    "incbin_missing_offset": '.incbin "payload.bin",.const(MISSING),1',
    "incbin_missing_size": '.incbin "payload.bin",0,.const(MISSING)',
    "incbin_query_only": '.incbin "payload.bin",.const(.ref(MISSING)),1',
    "label_include": 'HERE: .include .sprintf("selected%d.inc",.defined(HERE))',
    "label_incbin": 'HERE: .incbin "payload.bin",.defined(HERE),1',
}
BODIES.update(
    {
        "incbin_forward_const": '.incbin "payload.bin",.const(LATER),1\nLATER=2',
        "incbin_forward_value": '.incbin "payload.bin",LATER,1\nLATER=2',
        "incbin_alias": 'ALIAS=VALUE\n.incbin "payload.bin",.const(ALIAS),.ref(VALUE)',
        "incbin_scoped": '.scope INNER\n.incbin "payload.bin",.const(VALUE),1\n.byte .ref(VALUE)\n.endscope',
        "include_scoped": '.scope INNER\n.include .sprintf("selected%d.inc",.const(VALUE))\n.byte .ref(VALUE)\n.endscope',
        "include_name_query": '.include .sprintf("selected%d.inc",.ref(MISSING))',
        "incbin_name_query": '.incbin "payload.bin",.ref(MISSING),1',
        "incbin_zero_window": '.incbin "payload.bin",0,0',
        "incbin_end_offset": '.incbin "payload.bin",4,0',
        "incbin_large_offset": '.incbin "payload.bin",5,0',
        "incbin_negative_offset": '.incbin "payload.bin",-1,1',
        "incbin_negative_size": '.incbin "payload.bin",0,-1',
        "incbin_remainder": '.incbin "payload.bin",1',
        "incbin_all": '.incbin "payload.bin"',
        "include_tail": '.include "selected0.inc",.sprintf("%d",VALUE)',
        "incbin_extra": '.incbin "payload.bin",0,1,2',
        "include_label_cancel": 'HERE: .include .sprintf("selected%d.inc",.const(HERE-HERE))',
        "incbin_label_cancel": 'HERE: .incbin "payload.bin",.const(HERE-HERE),1',
        "include_cycle": 'FIRST=SECOND\nSECOND=FIRST\n.include .sprintf("selected%d.inc",.const(FIRST))',
        "incbin_cycle": 'FIRST=SECOND\nSECOND=FIRST\n.incbin "payload.bin",.const(FIRST),1',
    }
)
prepared = {}
for name, body in BODIES.items():
    body = "VALUE=2\n" + body + "\n.byte .ref(VALUE)\n"
    for context in ["direct", "included", "macro", "inactive", "unused"]:
        key = name + "_" + context
        if context == "included":
            FILES[key + ".inc"] = body
            source = f'.include "{key}.inc"\n'
        elif context in ["macro", "unused"]:
            source = (
                ".macro run\n"
                + body
                + ".endmacro\n"
                + ("run\n" if context == "macro" else ".byte 9\n")
            )
        elif context == "inactive":
            source = ".if 0\n" + body + ".endif\n.byte 9\n"
        else:
            source = body
        prepared[key] = source
assert len(prepared) == 200
spec = importlib.util.spec_from_file_location(
    "capture", ROOT / "docs/plans/2026-10-19-ca65-scope-binding/probe.py"
)
assert spec and spec.loader
capture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capture)
capture.cases = lambda: prepared
capture.FILES = FILES
capture.main()
