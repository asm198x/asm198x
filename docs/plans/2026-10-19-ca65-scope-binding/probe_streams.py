"""Retain stream ordering and instruction-size invalidation controls."""

import probe


def cases() -> dict[str, str]:
    body = ".scope\n.byte .defined(VALUE),VALUE,.defined(VALUE)\nVALUE=3\n.endscope\n"
    return {
        "included_scoped_label": '.scope inner\nentry:.include "labels.inc"\n.word entry,included\n.endscope\n',
        "included_alias_state": 'VALUE=2\n.scope inner\n.include "constants.inc"\n.byte OTHER\n.endscope\n',
        "macro_scoped_queries": "VALUE=2\n.macro emit\n"
        + body
        + ".endmacro\nemit\nemit\n",
        "repeat_scoped_queries": "VALUE=2\n.repeat 2\n" + body + ".endrepeat\n",
        "alias_size_changes": 'VALUE=$1234\n.scope inner\nOTHER=VALUE\nSECOND=OTHER+0\nlda SECOND\n.segment "ZEROPAGE"\nVALUE: .res 1\n.segment "CODE"\nlda SECOND\n.endscope\n',
        "alias_size_changes_on_label_line": 'VALUE=$1234\n.scope inner\nOTHER=VALUE\nlda OTHER\n.segment "ZEROPAGE"\nVALUE: lda OTHER\nafter: .res 1\n.segment "CODE"\n.word after\n.endscope\n',
    }


if __name__ == "__main__":
    probe.CONFIG = 'MEMORY { ZP:start=$20,size=$E0,file=""; MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { ZEROPAGE:load=ZP,type=zp; CODE:load=MAIN,type=ro; }\n'
    probe.FILES = {
        "labels.inc": "included: .byte 7\n",
        "constants.inc": "OTHER=VALUE\n.byte .defined(VALUE)\nVALUE=3\n",
    }
    probe.cases = cases
    probe.main()
