"""Capture scalar-consumer source ordering against native ca65/ld65."""
import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
FILES: dict[str, str] = {}


def cases() -> dict[str, str]:
    bodies: dict[str, str] = {}
    counts = {
        "constant": ".const(VALUE)",
        "order": ".ref(VALUE)+.const(VALUE)",
        "reverse": ".const(VALUE)+.ref(VALUE)",
        "ordinary": "VALUE",
        "nested": '.strat(.sprintf("%d",VALUE),0)-48',
        "query_only": ".const(.ref(MISSING))",
        "missing": ".const(MISSING)",
        "forward": ".const(LATER)",
        "zero_missing": ".const(MISSING*0)",
    }
    for name, value in counts.items():
        tail = "LATER=2\n" if name == "forward" else ""
        bodies["repeat_" + name] = f"VALUE=2\n.repeat {value}, INDEX\n.byte INDEX,.ref(VALUE)\n.endrepeat\n.byte .ref(VALUE)\n{tail}"
        for directive in ["res", "align"]:
            prefix = ".byte 7\n" if directive == "align" else ""
            bodies[directive + "_" + name] = f"VALUE=2\n{prefix}.{directive} {value},.ref(VALUE)\n.byte .ref(VALUE)\n{tail}"
    for directive in ["res", "align"]:
        for name, fill in {
            "fill_constant": ".const(VALUE)",
            "fill_order": ".ref(VALUE)+.const(VALUE)",
            "fill_value": "VALUE",
            "fill_nested": '.strat(.sprintf("%d",VALUE),0)',
            "fill_missing": ".const(MISSING)",
            "fill_range": "256",
        }.items():
            bodies[directive + "_" + name] = f"VALUE=2\n.{directive} 2,{fill}\n.byte .ref(VALUE)\n"
    bodies.update({
        "zero_res_fill": "VALUE=2\n.res 0,VALUE\n.byte .ref(VALUE)\n",
        "no_padding_align_fill": "VALUE=2\n.align 1,VALUE\n.byte .ref(VALUE)\n",
        "repeat_difference": "LEFT:.byte 0\nRIGHT:.byte 0\n.repeat .const(RIGHT-LEFT)\n.byte 7\n.endrepeat\n",
        "repeat_scoped": "VALUE=2\n.scope INNER\n.repeat .const(VALUE), INDEX\n.byte .ref(VALUE)\n.endrepeat\n.byte .ref(VALUE)\n.endscope\n",
        "repeat_cycle": "FIRST=SECOND\nSECOND=FIRST\n.repeat .const(FIRST)\n.byte 7\n.endrepeat\n",
        "assert_order": 'VALUE=2\n.assert .ref(VALUE)+.const(VALUE)=1,error,"order"\n.byte .ref(VALUE)\n',
        "assert_deferred": '.assert LATER=7,error,"later"\nLATER=7\n.byte 9\n',
        "assert_message": 'VALUE=2\n.assert 1,error,.sprintf("%d",VALUE)\n.byte .ref(VALUE)\n',
        "assert_message_order": 'VALUE=2\n.assert .const(VALUE),error,.sprintf("%d",.ref(VALUE))\n.byte .ref(VALUE)\n',
        "assert_message_failure": 'VALUE=2\n.assert 0,warning,.sprintf("%d",VALUE)\n.byte .ref(VALUE)\n',
        "out_number": 'VALUE=2\n.out .sprintf("%d",VALUE)\n.byte .ref(VALUE)\n',
        "out_nested": 'VALUE=2\n.out .concat("x",.sprintf("%d%d",.ref(VALUE),VALUE))\n.byte .ref(VALUE)\n',
        "out_selector": 'VALUE=2\n.out .left(50,{.sprintf("%d",VALUE)})\n.byte .ref(VALUE)\n',
        "out_tail": 'VALUE=2\n.out "plain",.sprintf("%d",VALUE)\n.byte .ref(VALUE)\n',
        "warning_tail": 'VALUE=2\n.warning "plain",.sprintf("%d",VALUE)\n.byte .ref(VALUE)\n',
        "warning_number": 'VALUE=2\n.warning .sprintf("%d",VALUE)\n.byte .ref(VALUE)\n',
        "error_number": 'VALUE=2\n.error .sprintf("%d",VALUE)\n.byte .ref(VALUE)\n',
    })
    prepared = {}
    for name, body in bodies.items():
        for context in ["direct", "included", "macro", "inactive", "unused"]:
            key = name + "_" + context
            if context == "included":
                FILES[key + ".inc"] = body
                source = f'.include "{key}.inc"\n'
            elif context in ["macro", "unused"]:
                source = ".macro run\n" + body + ".endmacro\n" + ("run\n" if context == "macro" else ".byte 9\n")
            elif context == "inactive":
                source = ".if 0\n" + body + ".endif\n.byte 9\n"
            else:
                source = body
            prepared[key] = source
    return prepared


if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location("capture", ROOT / "docs/plans/2026-10-19-ca65-scope-binding/probe.py")
    assert spec and spec.loader
    capture = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(capture)
    prepared = cases()
    capture.cases = lambda: prepared
    capture.FILES = FILES
    capture.main()
