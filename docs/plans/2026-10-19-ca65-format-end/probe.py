"""Retain native syntax-reader controls without running formatter semantics."""

import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    return {
        "zero_repeat": ".repeat 0\n.end\n.endrepeat\n.byte 8\n",
        "unused_macro": ".macro stop\n.end\n.endmacro\n.byte 8\n",
        "called_macro": ".macro stop\n.end\n.endmacro\n.byte 7\nstop\n.byte 8\n",
        "root_end": ".byte 7\n.end\n.byte 8\n",
        "root_end_oversized": ".byte 7\n.end\n.byte $100000000\n",
        "root_end_bad_function": '.byte 7\n.end\n.byte .sprintf("%bad",0)\n',
        "root_end_unclosed_if": ".byte 7\n.end\n.if 1\n.byte 8\n",
        "root_end_unclosed_macro": ".byte 7\n.end\n.macro pending\n",
        "root_end_unclosed_scope": ".byte 7\n.end\n.scope pending\n",
        "root_end_unclosed_record": ".byte 7\n.end\n.struct pending\n",
        "called_end_unclosed_if": ".macro stop\n.end\n.endmacro\n.byte 7\nstop\n.if 1\n",
        "called_end_unclosed_scope": ".macro stop\n.end\n.endmacro\n.byte 7\nstop\n.scope pending\n",
        "false_end": ".if 0\n.end\n.else\n.byte 7\n.endif\n.byte 8\n",
        "true_end": ".if 1\n.byte 7\n.end\n.endif\n.byte 8\n",
        "repeat_end": ".repeat 2\n.byte 7\n.end\n.endrepeat\n.byte 8\n",
        "comment_end": "; .end\n.byte 7 ; .end\n.byte 8\n",
        "quoted_end": '.asciiz ".end;:.if"\n.byte 8\n',
        "ordinary_labels": "entry:.byte 7\n@cheap:.byte 8\n: .word entry\n",
        "end_tail_colon": ".byte 7\n.end:.if 1\n.byte 8\n",
        "labelled_end": ".byte 7\nfinish:.end\n.byte 8\n",
        "define_end": ".define STOP .end\n.byte 7\nSTOP\n.byte 8\n",
        "define_end_bad_argument": '.define STOP .end\n.byte 7\nSTOP .sprintf("%bad",0)\n',
        "constant_runs": "SHORT=1\nLONG_NAME=$02\n.byte SHORT,LONG_NAME\n.end\nIGNORED=$100000000\n",
        "unused_macro_bad_capture": ".macro stop\n.end\n.byte $100000000\n.endmacro\n.byte 8\n",
        "zero_repeat_bad_capture": ".repeat 0\n.end\n.byte $100000000\n.endrepeat\n.byte 8\n",
        "macro_literal_end": '.macro say\n.asciiz ".end"\n.endmacro\nsay\n.byte 8\n',
    }


if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location(
        "capture", Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
    )
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
