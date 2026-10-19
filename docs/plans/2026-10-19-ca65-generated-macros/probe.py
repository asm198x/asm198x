"""Measure generated labels on macro definition/call heads without eager folding."""

import importlib.util
from pathlib import Path

FILES = {
    "body.inc": '.macro emit\n.byte 7\n.endmacro\n.ident("mark"):emit\n.word mark\n'
}


def definition(head: str, body: str = ".byte 7", args: str = "") -> str:
    return f"{head}:.macro emit {args}\n{body}\n.endmacro\nemit 9\n"


def invocation(head: str, body: str = ".byte 7", args: str = "") -> str:
    return f".macro emit {args}\n{body}\n.endmacro\n{head}:emit 9\n"


def cases() -> dict[str, str]:
    out = {}
    for cpu in ["6502", "4510"]:
        for name in ["mark", "A", "X", "Y", "F", "Z", "S", "SP", "LDA"]:
            for generated in [False, True]:
                head = f'.ident("{name}")' if generated else name
                for kind, build in [("definition", definition), ("call", invocation)]:
                    out[f"{cpu}_{name}_{generated}_{kind}"] = (
                        f'.setcpu "{cpu}"\n'
                        + build(head, args="arg")
                        + f'.word .ident("{name}")\n'
                    )
    for kind, build in [("definition", definition), ("call", invocation)]:
        for name, head in [
            ("concat", '.ident(.concat("ma", "rk"))'),
            ("sprintf", '.ident( .sprintf("row%d", 2) ) '),
            ("spaced", ' .ident ( "mark" ) '),
            ("anonymous", ":"),
            ("cheap", "@near"),
        ]:
            source = build(head, ".byte arg", "arg")
            if name == "anonymous":
                source = source.replace("::.macro", ":.macro").replace(
                    "::emit", ":emit"
                )
            if name == "cheap":
                source = "anchor:.byte 6\n" + source + ".word @near\n"
            out[f"{kind}_{name}"] = source
    out.update(
        {
            "definition_phase": '.byte 6\n.ident("mark"):.macro emit\n.byte .defined(mark),.const(mark)\n.endmacro\n.byte .defined(mark)\nemit\n.word mark\n',
            "call_phase": '.byte 6\n.macro emit\n.byte .defined(mark),.const(mark)\n.endmacro\n.ident("mark"):emit\n.word mark\n',
            "empty_definition": '.ident("mark"):.macro emit\n.endmacro\nemit\n.word mark\n',
            "empty_call": '.macro emit\n.endmacro\n.ident("mark"):emit\n.word mark\n',
            "unknown_definition_head_skipped": '.if 0\n.ident(.sprintf("%bad",0)):.macro emit\n.byte 7\n.endmacro\n.endif\n.byte 8\n',
            "unknown_call_head_skipped": '.macro emit\n.byte 7\n.endmacro\n.if 0\n.ident(.sprintf("%bad",0)):emit\n.endif\n.byte 8\n',
            "unused_nested_definition": '.macro outer\n.ident(.sprintf("%bad",0)):.macro emit\n.byte 7\n.endmacro\n.endmacro\n.byte 8\n',
            "zero_repeated_definition": '.repeat 0\n.ident(.sprintf("%bad",0)):.macro emit\n.byte 7\n.endmacro\n.endrepeat\n.byte 8\n',
            "iteration_calls": '.macro emit arg\n.byte arg\n.endmacro\n.repeat 2,I\n.ident(.sprintf("row%d",I)):emit I\n.endrepeat\n.word row0,row1\n',
            "nested_call": '.macro emit\n.byte 7\n.endmacro\n.macro outer\n.ident("mark"):emit\n.endmacro\nouter\n.word mark\n',
            "scope_context": '.macro emit\n.byte 7\n.endmacro\n.scope outer\n.ident("mark"):emit\n.endscope\n.word outer::mark\n',
            "include_context": '.include "body.inc"\n',
            "captured_head": '.define NAME "before"\n.macro outer\n.ident(NAME):.macro emit\n.byte 7\n.endmacro\n.endmacro\n.undefine NAME\n.define NAME "after"\nouter\nemit\n.word before\n',
            "selected_bad_type": ".ident(7):.macro emit\n.endmacro\n.byte 8\n",
            "selected_bad_call_type": ".macro emit\n.endmacro\n.ident(7):emit\n",
            "selected_bad_name": '.ident("bad;name"):.macro emit\n.endmacro\n',
            "oversized_inactive": '.if 0\n.ident(.sprintf("%d",$100000000)):.macro emit\n.endmacro\n.endif\n',
            "duplicate_call": '.macro emit\n.byte 7\n.endmacro\nmark:.byte 6\n.ident("mark"):emit\n',
            "definition_reference_before_call": '.ident("mark"):.macro emit\n.byte 7\n.endmacro\n.word mark\nemit\n',
            "argument_text": '.define VALUE 9\n.macro emit arg\n.byte arg\n.endmacro\n.ident("mark"):emit VALUE\n.word mark\n',
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
