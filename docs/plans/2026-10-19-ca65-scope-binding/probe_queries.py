"""Retain name-query source order and scoped allocation controls."""

import probe


def cases() -> dict[str, str]:
    out = {}
    for name, body in {
        "query_before_value": ".byte .defined(VALUE),VALUE,.defined(VALUE)\n",
        "query_after_value": ".byte VALUE,.defined(VALUE)\n",
        "ref_before_value": ".byte .ref(VALUE),VALUE,.ref(VALUE)\n",
        "alias_allocates": "OTHER=VALUE\n.byte .defined(VALUE),.defined(OTHER),.ref(VALUE),.ref(OTHER)\n.byte OTHER\n",
        "self_definition_query": "VALUE=.defined(VALUE)\n.byte VALUE\n",
        "if_defined_order": ".ifdef VALUE\n.byte 7\n.endif\n.byte VALUE\n.ifdef VALUE\n.byte 8\n.endif\n",
        "if_ref_order": ".ifref VALUE\n.byte 7\n.endif\n.byte VALUE\n.ifref VALUE\n.byte 8\n.endif\n",
        "if_const_allocates": ".ifconst VALUE\n.byte 7\n.endif\n.byte .defined(VALUE),.ref(VALUE)\n",
        "root_ifdef": ".ifdef ::VALUE\n.byte 7\n.endif\n",
        "root_ifref": ".byte ::VALUE\n.ifref ::VALUE\n.byte 7\n.endif\n",
        "label_query": "target: .byte .defined(target)\n",
    }.items():
        out[name] = "VALUE=2\n.scope inner\n" + body + ".endscope\n"
    for name, source in {
        "root_value_ref": "VALUE=2\n.byte VALUE,.ref(VALUE)\n",
        "root_value_after_ref": "VALUE=2\n.byte .ref(VALUE),VALUE\n.byte .ref(VALUE)\n",
        "root_alias_ref": "VALUE=2\nOTHER=VALUE\n.byte .ref(VALUE),.ref(OTHER)\n",
        "root_alias_use_ref": "VALUE=2\nOTHER=VALUE\n.byte OTHER\n.byte .ref(VALUE),.ref(OTHER)\n",
        "root_const_ref": "VALUE=2\n.byte .ref(VALUE),.const(VALUE),.ref(VALUE)\n",
        "scope_known_ref": ".scope inner\nVALUE=2\n.byte VALUE,.ref(VALUE)\n.endscope\n",
        "root_expression_ref": "VALUE=2\n.byte VALUE+.ref(VALUE)\n",
        "root_expression_before_ref": "VALUE=2\n.byte .ref(VALUE)+VALUE\n",
        "cheap_scoped_fold": "anchor: .byte 0\n@COUNT=2\n.scope inner\n.res @COUNT\n.if @COUNT=2\n.byte 7\n.endif\n.byte .defined(@COUNT),.ref(@COUNT)\n.endscope\n",
        "root_record_count": "COUNT=2\n.struct Record\nfield .word COUNT\n.endstruct\n.word .sizeof(Record)\n",
        "scoped_record_count": ".scope inner\nCOUNT=2\n.struct Record\nfield .word COUNT\n.endstruct\n.word .sizeof(Record)\n.endscope\n",
        "outer_record_count": "COUNT=2\n.scope inner\n.struct Record\nfield .word COUNT\n.endstruct\n.word .sizeof(Record)\n.endscope\n",
        "enum_previous": ".enum Colors\nRED=2\nGREEN=RED+1\n.endenum\n.byte Colors::GREEN\n",
        "enum_self_parent": "RED=9\n.enum Colors\nRED=RED+1\n.endenum\n.byte Colors::RED\n",
        "enum_self_query": "RED=9\n.enum Colors\nRED=.defined(RED)\n.endenum\n.byte Colors::RED\n",
        "record_size_scope": ".struct Record\nfield .word\n.endstruct\n.scope inner\n.res .sizeof(Record)\n.endscope\n",
        "record_name_query": ".struct Record\nfield .word\n.endstruct\n.byte .defined(Record),.ref(Record)\n.word .sizeof(Record)\n.byte .defined(Record),.ref(Record)\n",
        "enum_reference": "COUNT=2\n.enum Colors\nRED=COUNT\n.endenum\n.byte .ref(COUNT)\n",
        "cheap_label_size": "anchor: .byte 0\n@local: .word 7\n.scope inner\n.res .sizeof(@local)\n.endscope\n",
        "enum_outer_count": "COUNT=2\n.enum Colors\nRED=COUNT\n.endenum\n.byte Colors::RED\n",
        "query_function_argument": "value: .byte 7\n.byte .defined(.sizeof(value))\n",
        "ref_function_argument": "value: .byte 7\n.byte .ref(.sizeof(value))\n",
        "query_query_argument": "VALUE=2\n.byte .defined(.ref(VALUE))\n",
        "masked_negative": "lda $FFFF\n.org $1000\ntarget: .byte 7\nbne target\n",
    }.items():
        out[name] = source
    return out


if __name__ == "__main__":
    probe.cases = cases
    probe.main()
