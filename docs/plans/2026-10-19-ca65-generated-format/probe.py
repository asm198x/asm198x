"""Measure generated-label source forms independently of the formatter."""

import importlib.util
from pathlib import Path

FILES = {"child.inc": '.ident("incmark"):.byte 7\n'}


def cases() -> dict[str, str]:
    out = {}
    for name in ["A", "X", "Y", "Z", "F"]:
        out[f"plain_{name}"] = f'.ident("{name}"): .byte 7\n'
        out[f"reference_{name}"] = f'.ident("{name}"):.byte 7\n.word .ident("{name}")\n'
        out[f"spaced_{name}"] = (
            f' ; source comment\n .ident( "{name}" ) : .byte 7 ; trailing .end\n'
        )
    out.update(
        {
            "concat_head": '.ident(.concat("fin", "ish")):.byte 7\n.word finish\n',
            "sprintf_head": '.ident( .sprintf("mark%d", 2) ) : .byte 7\n.word mark2\n',
            "scoped_head": '.scope outer\n.ident("finish"):.byte 7\n.endscope\n.word outer::finish\n',
            "scoped_register": '.scope outer\n.ident("A"):.byte 7\n.endscope\n.word outer::.ident("A")\n',
            "cheap_context": '.ident("anchor"):.byte 7\n@near:.byte 8\n.word @near\n',
            "conditional_head": '.ident("mark"):.if 1\n.byte 7\n.endif\n.word mark\n',
            "inactive_head": '.ident("mark"):.if 0\n.byte 7\n.endif\n.byte .defined(mark)\n',
            "repeat_head": '.ident("mark"):.repeat 2\n.byte 7\n.endrepeat\n.word mark\n',
            "macro_definition_head": '.ident("mark"):.macro emit\n.byte 7\n.endmacro\nemit\n.word mark\n',
            "macro_call_head": '.macro emit\n.byte 7\n.endmacro\n.ident("mark"):emit\n.word mark\n',
            "macro_body_head": '.macro emit\n.ident("mark"):.byte 7\n.endmacro\nemit\n.word mark\n',
            "scope_directive_head": '.ident("mark"):.scope outer\n.byte 7\n.endscope\n.word mark\n',
            "static_scope_directive_head": "mark:.scope outer\n.byte 7\n.endscope\n.word mark\n",
            "proc_directive_head": '.ident("mark"):.proc outer\n.byte 7\n.endproc\n.word mark\n',
            "static_conditional_head": "mark:.if 1\n.byte 7\n.endif\n.word mark\n",
            "static_repeat_head": "mark:.repeat 2\n.byte 7\n.endrepeat\n.word mark\n",
            "static_macro_definition_head": "mark:.macro emit\n.byte 7\n.endmacro\nemit\n.word mark\n",
            "static_macro_call_head": ".macro emit\n.byte 7\n.endmacro\nmark:emit\n.word mark\n",
            "static_proc_directive_head": "mark:.proc outer\n.byte 7\n.endproc\n.word mark\n",
            "include_context": '.include "child.inc"\n.word incmark\n',
            "unread_missing_include": '.end\n.include "missing.inc"\n.ident("A"):.byte 7\n',
            "missing_include": '.include "missing.inc"\n.ident("A"):.byte 7\n',
            "invalid_numeric_argument": ".ident(7):.byte 7\n",
            "invalid_unknown_argument": ".ident(UNKNOWN):.byte 7\n",
            "invalid_generated_name": '.ident("bad;name"):.byte 7\n',
            "invalid_format_argument": '.ident(.sprintf("%bad",0)):.byte 7\n',
            "unclosed_argument": '.ident("mark":.byte 7\n',
            "static_baseline": "mark:.byte 7\n.word mark\n",
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
    runner.cases = cases
    runner.FILES = FILES
    runner.main()
