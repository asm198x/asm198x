import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
FILES = {"payload.bin": "ABCD"}
bodies = {
    "repeat_fields": ".repeat .left(3,{2,INDEX})\n.byte INDEX\n.endrepeat\n",
    "res_fields": ".res .left(3,{1,2})\n",
    "align_fields": ".byte 7\n.align .left(3,{2,3})\n",
    "assert_fields": '.assert .left(3,{1,error}),"ok"\n.byte 9\n',
    "incbin_fields": '.incbin "payload.bin",.left(3,{1,2})\n',
    "res_suffix": ".res .left(3,{1,2})+1\n",
    "repeat_suffix": ".repeat .left(3,{1,INDEX})\n.byte INDEX\n.endrepeat\n",
    "res_read_order": "VALUE=2\n.res .left(3,{.const(VALUE),.ref(VALUE)})\n.byte .ref(VALUE)\n",
}
prepared = {}
for name, body in bodies.items():
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
spec = importlib.util.spec_from_file_location(
    "capture", ROOT / "docs/plans/2026-10-19-ca65-scope-binding/probe.py"
)
assert spec and spec.loader
c = importlib.util.module_from_spec(spec)
spec.loader.exec_module(c)
c.cases = lambda: prepared
c.FILES = FILES
c.main()
