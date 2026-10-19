"""Pin logical line counters separately from physical scope spans."""

import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "capture", Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
)
assert spec is not None and spec.loader is not None
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
runner.CONFIG = 'MEMORY { MAIN:start=$1000,size=$7000,file=%O; RAM:start=$20,size=$100,file=""; }\nSEGMENTS { CODE:load=MAIN,type=ro; RODATA:load=MAIN,type=ro; BSS:load=RAM,type=bss; ZEROPAGE:load=RAM,type=zp; }\n'
runner.FILES = {"bare.inc": ".byte 7\n.word 8\n"}


def cases() -> dict[str, str]:
    out = {
        "org_label_address": "item:.org $2000\n.word item\n",
        "absolute_org_label_address": ".org $2000\nitem:.org $3000\n.word item\n",
        "reloc_label_address": ".org $2000\nitem:.reloc\n.word item\n",
        "org_pc_relative": "item:.org *+3\n.word .sizeof(item)\n",
        "absolute_pc_relative": ".org $2000\nitem:.org *+3\n.word .sizeof(item)\n",
        "org_forward": "item:.org DEST\nDEST=$2000\n.word .sizeof(item)\n",
        "org_delta_after_bytes": ".byte 7\nitem:.org $2000\n.dword .sizeof(item)\n",
        "org_delta_absolute": ".org $2000\nitem:.org $3000\n.dword .sizeof(item)\n",
        "org_delta_backward": ".org $3000\nitem:.org $2000\n.dword .sizeof(item)\n",
        "reloc_delta": ".org $2000\nitem:.reloc\n.dword .sizeof(item)\n",
        "segment_line_size": 'item:.segment "RODATA"\n.byte 7\n.word .sizeof(item)\n',
        "include_line_size": 'item:.include "bare.inc"\n.word .sizeof(item)\n',
        "repeat_line_size": "item:.repeat 3\n.byte 7\n.endrepeat\n.word .sizeof(item)\n",
        "conditional_line_size": "item:.if 1\n.byte 7\n.endif\n.word .sizeof(item)\n",
        "label_on_endproc": ".proc item\n.byte 7\nlast:.endproc\n.word .sizeof(item),.sizeof(item::last)\n",
        "empty_scope": ".scope inner\n.endscope\n.word .sizeof(inner)\n",
        "scope_only_other_segment": '.scope inner\n.segment "RODATA"\n.byte 7,8\n.endscope\n.word .sizeof(inner)\n',
        "procedure_only_other_segment": '.proc item\n.segment "RODATA"\n.byte 7,8\n.endproc\n.word .sizeof(item)\n',
        "scope_initial_later": '.scope inner\n.segment "RODATA"\n.byte 7,8\n.segment "CODE"\n.byte 9\n.endscope\n.word .sizeof(inner)\n',
        "scope_existing_segment_order": '.segment "RODATA"\n.byte 1\n.segment "BSS"\n.res 1\n.segment "CODE"\n.scope inner\n.segment "BSS"\n.res 3\n.segment "RODATA"\n.byte 7,8\n.endscope\n.segment "CODE"\n.word .sizeof(inner)\n',
    }
    return out


runner.cases = cases
runner.main()
