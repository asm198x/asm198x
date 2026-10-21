"""Measure the next #795 consumers without modifying the qualified source."""
import importlib.util
from pathlib import Path

repo = Path(__file__).resolve().parents[3]
files = {"payload.bin": "ABCD", "selected2.inc": ".byte .ref(VALUE)\n"}
bodies = {
    "repeat_const": "VALUE=2\n.repeat .const(VALUE)\n.byte .ref(VALUE)\n.endrepeat\n.byte .ref(VALUE)\n",
    "repeat_order": "VALUE=2\n.repeat .ref(VALUE)+.const(VALUE)\n.byte .ref(VALUE)\n.endrepeat\n.byte .ref(VALUE)\n",
    "repeat_difference": "LEFT:.byte 0\nRIGHT:.byte 0\n.repeat .const(RIGHT-LEFT)\n.byte 7\n.endrepeat\n",
    "reserve_fill": "VALUE=2\n.res .const(VALUE),.ref(VALUE)\n.byte .ref(VALUE)\n",
    "reserve_order": "VALUE=2\n.res .ref(VALUE)+.const(VALUE),.ref(VALUE)\n.byte .ref(VALUE)\n",
    "align_fill": "VALUE=2\n.byte 7\n.align .const(VALUE)+1,.ref(VALUE)\n.byte .ref(VALUE)\n",
    "assert_order": 'VALUE=2\n.assert .ref(VALUE)+.const(VALUE)=1,error,"order"\n.byte .ref(VALUE)\n',
    "out_number": 'VALUE=2\n.out .sprintf("%d",VALUE)\n.byte .ref(VALUE)\n',
    "incbin_bounds": 'VALUE=2\n.incbin "payload.bin",.const(VALUE),.ref(VALUE)\n.byte .ref(VALUE)\n',
    "include_name": 'VALUE=2\n.include .sprintf("selected%d.inc",VALUE)\n',
}
prepared = {}
for name, body in bodies.items():
    for context in ["direct", "included", "macro", "inactive", "unused"]:
        key = name + "_" + context
        if context == "included":
            files[key + ".inc"] = body
            source = f'.include "{key}.inc"\n'
        elif context in ["macro", "unused"]:
            source = ".macro run\n" + body + ".endmacro\n" + ("run\n" if context == "macro" else ".byte 9\n")
        elif context == "inactive":
            source = ".if 0\n" + body + ".endif\n.byte 9\n"
        else:
            source = body
        prepared[key] = source
assert len(prepared) == 50
spec = importlib.util.spec_from_file_location("capture", repo / "docs/plans/2026-10-19-ca65-scope-binding/probe.py")
assert spec is not None and spec.loader is not None
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
runner.FILES = files
runner.cases = lambda: prepared
runner.main()
