"""Capture enum expression/cursor behavior with bounded native processes."""

import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {
        "defaults": ".enum Colors\nRED\nGREEN\nBLUE\n.endenum\n.byte Colors::RED,Colors::GREEN,Colors::BLUE\n",
        "previous": ".enum Colors\nRED=2\nGREEN=RED+1\nBLUE\n.endenum\n.byte Colors::RED,Colors::GREEN,Colors::BLUE\n",
        "forward": ".enum Colors\nRED=GREEN+1\nBLUE\nGREEN=4\nWHITE\n.endenum\n.byte Colors::RED,Colors::BLUE,Colors::GREEN,Colors::WHITE\n",
        "outer_fallback": "VALUE=2\n.enum Colors\nRED=VALUE\nGREEN\n.endenum\n.byte Colors::RED,Colors::GREEN\n",
        "local_shadow": "VALUE=2\n.enum Colors\nRED=VALUE\nGREEN\nVALUE=4\n.endenum\n.byte Colors::RED,Colors::GREEN\n",
        "root_explicit": "VALUE=2\n.enum Colors\nRED=::VALUE\nGREEN\nVALUE=4\n.endenum\n.byte Colors::RED,Colors::GREEN\n",
        "outer_self_cycle": "RED=9\n.enum Colors\nRED=RED+1\n.endenum\n.byte 7\n",
        "cycle_unused": ".enum Colors\nRED=GREEN\nGREEN=RED\n.endenum\n.byte 7\n",
        "undefined_unused": ".enum Colors\nRED=MISSING\n.endenum\n.byte 7\n",
        "defined_self": "RED=9\n.enum Colors\nRED=.defined(RED)\nGREEN=.defined(RED)\n.endenum\n.byte Colors::RED,Colors::GREEN\n",
        "queries": "VALUE=2\n.enum Colors\nRED=.defined(VALUE)\nGREEN=VALUE\nBLUE=.defined(VALUE)\nWHITE=.ref(VALUE)\nVALUE=4\n.endenum\n.byte Colors::RED,Colors::GREEN,Colors::BLUE,Colors::WHITE\n",
        "source_fold_inside": ".enum Colors\nRED=2\n.if RED=2\nGREEN=3\n.endif\n.endenum\n.byte Colors::RED,Colors::GREEN\n",
        "source_fold_after": ".enum Colors\nRED=GREEN\nGREEN=3\n.endenum\n.res Colors::RED\n",
        "source_fold_later_outer": ".enum Colors\nRED=VALUE\n.endenum\nVALUE=3\n.res Colors::RED\n",
        "source_fold_unresolved": "VALUE=2\n.enum Colors\nRED=VALUE\n.endenum\n.res Colors::RED\n",
        "global_alias": ".enum Colors\nRED=GREEN\nGREEN=3\n.endenum\nOTHER=Colors::RED\n.byte OTHER\n",
        "label": "target: .byte 7\n.enum Colors\nRED=target\nGREEN\n.endenum\n.word Colors::RED,Colors::GREEN\n",
        "pc": ".byte 7\n.enum Colors\nRED=*\nGREEN\n.endenum\n.word Colors::RED,Colors::GREEN\n",
        "nested_enum": ".enum Outer\nRED=2\n.enum Inner\nGREEN=RED\n.endenum\nBLUE\n.endenum\n.byte Outer::RED,Outer::BLUE,Outer::Inner::GREEN\n",
        "anonymous": "VALUE=2\n.enum\nRED=VALUE\nGREEN\nVALUE=4\n.endenum\n.byte RED,GREEN,VALUE\n",
        "nested_scope": "VALUE=2\n.scope inner\n.enum Colors\nRED=VALUE\nGREEN\nVALUE=4\n.endenum\n.byte Colors::RED,Colors::GREEN\n.endscope\n",
        "size_empty": ".enum Colors\n.endenum\n.word .sizeof(Colors)\n",
        "size_defaults": ".enum Colors\nRED\nGREEN\n.endenum\n.word .sizeof(Colors),.sizeof(Colors::RED),.sizeof(Colors::GREEN)\n",
        "size_forward": ".enum Colors\nRED=GREEN\nGREEN=4\nBLUE\n.endenum\n.word .sizeof(Colors),.sizeof(Colors::RED),.sizeof(Colors::BLUE)\n",
        "size_reset": ".enum Colors\nRED=100\nGREEN=2\nBLUE\n.endenum\n.word .sizeof(Colors)\n",
        "duplicate": ".enum Colors\nRED=2\nRED=3\n.endenum\n.byte 7\n",
        "cheap_anchor": "root:\n@value=2\n.enum Colors\nRED=@value\nGREEN\n.endenum\n.byte Colors::RED,Colors::GREEN\n",
        "record_coexist": ".struct Record\nfield .word\n.endstruct\n.enum Colors\nRED=.sizeof(Record)\nGREEN\n.endenum\n.byte Colors::RED,Colors::GREEN\n",
    }
    for value in [
        "-2",
        "-1",
        "0",
        "255",
        "256",
        "65535",
        "65536",
        "$7FFFFFFF",
        "$FFFFFFFF",
        "4294967296",
    ]:
        for suffix, use in [
            ("values", ".dword Colors::RED,Colors::GREEN"),
            ("size", ".dword .sizeof(Colors)"),
        ]:
            out[f"boundary_{value}_{suffix}"] = (
                f".enum Colors\nRED={value}\nGREEN\n.endenum\n{use}\n"
            )
    return out


if __name__ == "__main__":
    source = Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
    spec = importlib.util.spec_from_file_location("native_capture", source)
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
