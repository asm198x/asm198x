"""Retain separate constancy, enum and size-namespace findings."""

import probe


def cases() -> dict[str, str]:
    return {
        "const_global_refs": "VALUE=2\n.byte .ref(VALUE),.const(VALUE),.ref(VALUE)\n",
        "const_scoped_order": "VALUE=2\n.scope inner\n.byte .defined(VALUE),.const(VALUE),.defined(VALUE),.ref(VALUE)\n.endscope\n",
        "const_scoped_known": ".scope inner\nVALUE=2\n.byte .const(VALUE)\n.endscope\n",
        "const_scoped_root": "VALUE=2\n.scope inner\n.byte .const(::VALUE)\n.endscope\n",
        "const_nested_text": 'VALUE=2\n.byte .ref(VALUE),.sprintf("%d",.const(VALUE)),.ref(VALUE)\n',
        "numeric_text_refs": 'VALUE=2\n.byte .ref(VALUE),.sprintf("%d",VALUE),.ref(VALUE)\n',
        "alias_difference_constancy": "first: .byte 0\nsecond: .byte 0\nOTHER=second-first\n.ifconst OTHER\n.byte 7\n.endif\n",
        "enum_forward_shadow": "VALUE=2\n.enum Colors\nRED=VALUE\nVALUE=3\n.endenum\n.byte Colors::RED\n",
        "enum_previous_member": ".enum Colors\nRED=2\nGREEN=RED+1\n.endenum\n.byte Colors::GREEN\n",
        "enum_self_parent": "RED=9\n.enum Colors\nRED=RED+1\n.endenum\n.byte Colors::RED\n",
        "enum_self_query": "RED=9\n.enum Colors\nRED=.defined(RED)\n.endenum\n.byte Colors::RED\n",
        "record_size_cheap_constant": ".struct Record\nfield .word\n.endstruct\nRecord=0\n@size=7\n.byte @size\n",
        "record_size_cheap_label": ".struct Record\nfield .word\n.endstruct\nRecord: .byte 0\n@size: .byte 7\n",
        "proc_size_cheap_constant": ".proc start\n.byte 0\n@size=7\n.byte @size\n.endproc\n.word .sizeof(start)\n",
        "proc_size_cheap_label": ".proc start\n.byte 0\n@size: .byte 7\n.endproc\n.word .sizeof(start)\n",
        "proc_size_large_body": ".proc start\n.res 256\n@size=7\n.byte @size\n.endproc\n.word .sizeof(start)\n",
        "size_cheap_definition": "start: .byte 0\n@size=7\n.byte @size\n",
        "size_cheap_undefined": "start: .byte 0\n.byte @size\n",
        "size_cheap_query": "start: .byte 0\n.byte .defined(@size),.ref(@size)\n",
        "size_forward_label": ".word .sizeof(later)\nlater: .byte 7\n",
        "size_scoped_forward": ".scope inner\n.word .sizeof(later)\nlater: .byte 7\n.endscope\n",
        "size_outer_label_shadow": "later: .byte 7\n.scope inner\n.word .sizeof(later)\nlater: .word 9\n.endscope\n",
    }


if __name__ == "__main__":
    probe.cases = cases
    probe.main()
