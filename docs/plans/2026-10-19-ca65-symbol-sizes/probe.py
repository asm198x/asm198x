"""Measure size attribute availability and physical/source placement."""

import importlib.util
from pathlib import Path

BODIES = {
    "byte": ".byte 7\n",
    "word": ".word 7\n",
    "dword": ".dword 7\n",
    "dbyt": ".dbyt 7\n",
    "string": '.byte "ABC"\n',
    "empty_string": '.byte ""\n',
    "reserve": ".res 3\n",
    "filled_reserve": ".res 3,$AA\n",
    "instruction": "nop\n",
    "short_instruction": "lda $12\n",
    "long_instruction": "lda $1234\n",
    "many_lines": ".byte 7\n.word 8\n",
    "alignment": ".res 1\n.align 4\n",
    "explicit_alignment": ".res 1\n.align 4,$AA\n",
    "org": ".org $2000\n.byte 7\n",
    "empty": "",
    "binary": '.incbin "blob.bin"\n',
}


def cases() -> dict[str, str]:
    out = {}
    for name, body in BODIES.items():
        out["label_" + name] = "item: " + body + "\n.word .sizeof(item)\n"
        out["cheap_" + name] = "head:\n@item: " + body + "\n.word .sizeof(@item)\n"
        out["procedure_" + name] = (
            ".proc item\n" + body + ".endproc\n.word .sizeof(item)\n"
        )
    out.update(
        {
            "label_own_operand": "item:.word .sizeof(item)\n",
            "label_forward": ".word .sizeof(item)\nitem:.byte 7\n",
            "label_separate_line": "item:\n.byte 7\n.word .sizeof(item)\n",
            "label_two_on_one_line": "first: second:.byte 7\n.word .sizeof(first),.sizeof(second)\n",
            "label_alias": "item:.byte 7\nALIAS=item\n.word .sizeof(ALIAS)\n",
            "constant": "VALUE=2\n.word .sizeof(VALUE)\n",
            "procedure_own": ".proc item\n.word .sizeof(item)\n.endproc\n",
            "procedure_forward": ".word .sizeof(item)\n.proc item\n.byte 7\n.endproc\n",
            "procedure_large": ".proc item\n.res 257\n.endproc\n.word .sizeof(item)\n",
            "procedure_alias": ".proc item\n.byte 7\n.endproc\nALIAS=item\n.word .sizeof(ALIAS)\n",
            "scope": ".scope inner\n.byte 7\n.endscope\n.word .sizeof(inner)\n",
            "scope_label": "item:.scope inner\n.byte 7\n.endscope\n.word .sizeof(item)\n",
            "outer_before_local": "item:.byte 7\n.scope inner\n.word .sizeof(item)\nitem:.word 9\n.word .sizeof(item)\n.endscope\n",
            "qualified_label": ".scope inner\nitem:.byte 7\n.endscope\n.word .sizeof(inner::item)\n",
            "qualified_forward": ".word .sizeof(inner::item)\n.scope inner\nitem:.byte 7\n.endscope\n",
            "reservation_query": "item:.word 7\n.res .sizeof(item),$AA\n",
            "cheap_reservation": "head:\n@item:.word 7\n.res .sizeof(@item),$AA\n",
            "bss_label": '.segment "BSS"\nitem:.res 3\n.segment "CODE"\n.word .sizeof(item)\n',
            "zp_label": '.segment "ZEROPAGE"\nitem:.res 3\n.segment "CODE"\n.word .sizeof(item)\n',
            "procedure_segment_return": '.proc item\n.byte 1\n.segment "RODATA"\n.byte 2,3\n.segment "CODE"\n.byte 4\n.endproc\n.word .sizeof(item)\n',
            "procedure_segment_leave": '.proc item\n.byte 1\n.segment "RODATA"\n.byte 2,3\n.endproc\n.segment "CODE"\n.word .sizeof(item)\n',
            "procedure_reloc": ".proc item\n.byte 1\n.org $2000\n.byte 2\n.reloc\n.byte 3\n.endproc\n.word .sizeof(item)\n",
            "procedure_nested": ".proc item\n.byte 1\n.proc child\n.word 2\n.endproc\n.byte 3\n.endproc\n.word .sizeof(item),.sizeof(item::child)\n",
            "include": '.include "data.inc"\n.word .sizeof(item)\n',
            "macro": ".macro make name\nname:.word 7\n.word .sizeof(name)\n.endmacro\nmake item\n",
            "repeat": '.repeat 3,I\n.ident(.sprintf("L%d",I)):.byte I\n.word .sizeof(.ident(.sprintf("L%d",I)))\n.endrepeat\n',
            "reference_effect": "item:.byte 7\n.byte .referenced(item)\n.word .sizeof(item)\n.byte .referenced(item)\n",
            "private_property": "item:.byte 7\n@size=9\n.word .sizeof(item),@size\n",
            "missing": ".word .sizeof(missing)\n",
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
    runner.CONFIG = 'MEMORY { MAIN:start=$1000,size=$7000,file=%O; RAM:start=$20,size=$100,file=""; }\nSEGMENTS { CODE:load=MAIN,type=ro; RODATA:load=MAIN,type=ro; BSS:load=RAM,type=bss; ZEROPAGE:load=RAM,type=zp; }\n'
    runner.FILES = {"blob.bin": "ABC", "data.inc": "item:.byte 7\n"}
    runner.cases = cases
    runner.main()
