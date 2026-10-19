"""Pin enum token context, source streams and captured default bases."""

import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {
        "enum_include": 'VALUE=2\n.include "enums.inc"\n.byte Colors::RED,Colors::GREEN\n',
        "enum_include_after_macro": '.macro load_colors\n.include "enums.inc"\n.endmacro\nVALUE=2\nload_colors\n.byte Colors::RED,Colors::GREEN\n',
        "enum_macro_fields": ".macro color_fields\nRED=2\nGREEN\n.endmacro\n.enum Colors\ncolor_fields\n.endenum\n.byte Colors::RED,Colors::GREEN\n",
        "enum_macro_namespace": ".macro make_colors name,value\n.enum name\nRED=value\nGREEN\n.endenum\n.byte name::RED,name::GREEN\n.endmacro\nmake_colors Colors,2\n",
        "enum_repeated_namespace": '.repeat 2,I\n.enum .ident(.sprintf("Colors_%d",I))\nRED=I+2\nGREEN\n.endenum\n.endrepeat\n.byte Colors_0::RED,Colors_0::GREEN,Colors_1::RED,Colors_1::GREEN\n',
        "enum_captured_query_base": "VALUE=2\n.enum Colors\nRED=.defined(VALUE)\nGREEN\nVALUE=4\nBLUE\n.endenum\n.byte Colors::RED,Colors::GREEN,Colors::VALUE,Colors::BLUE\n",
        "enum_constancy_updates": "VALUE=2\n.enum Colors\nRED=VALUE\n.ifconst RED\nGREEN=7\n.else\nGREEN=9\n.endif\nVALUE=4\nBLUE\n.endenum\n.byte Colors::RED,Colors::GREEN,Colors::BLUE\n",
        "enum_closing_cheap_anchor": "root:\n.enum Colors\nRED=2\n.endenum\n@cheap=3\n.byte @cheap\n",
        "enum_include_inside_refuses": '.enum Colors\n.include "enums.inc"\n.endenum\n.byte 7\n',
        "enum_reset_cursor": ".enum Colors\nRED=LATE\nGREEN\nBLUE=10\nWHITE\nLATE=4\nBLACK\n.endenum\n.byte Colors::RED,Colors::GREEN,Colors::BLUE,Colors::WHITE,Colors::LATE,Colors::BLACK\n",
    }
    for extended in [False, True]:
        cpu = ".p4510\n" if extended else ""
        for name in ["A", "X", "Y", "LDA", "RED", "SP", "Z", "BRU"]:
            out[f"enum_member_token_{extended}_{name}"] = (
                cpu + f".enum Colors\n{name}=2\n.endenum\n.byte 7\n"
            )
            out[f"enum_header_token_{extended}_{name}"] = (
                cpu + f".enum {name}\nRED=2\n.endenum\n.byte 7\n"
            )
    return out


if __name__ == "__main__":
    source = Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
    spec = importlib.util.spec_from_file_location("native_capture", source)
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.FILES = {"enums.inc": ".enum Colors\nRED=VALUE\nGREEN\nVALUE=4\n.endenum\n"}
    runner.cases = cases
    runner.main()
