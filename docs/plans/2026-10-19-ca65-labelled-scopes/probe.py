"""Measure labels on scope/procedure openings in their enclosing namespace."""
import importlib.util
from pathlib import Path

FILES = {"body.inc": '.ident("mark"):.scope inner\n.byte 7\n.endscope\n.word mark\n'}

def cases() -> dict[str, str]:
    out = {}
    for cpu in ["6502", "4510"]:
        for name in ["mark", "A", "X", "Y", "F", "Z", "S", "SP", "LDA"]:
            for generated in [False, True]:
                head = f'.ident("{name}")' if generated else name
                for directive, close in [(".scope inner", ".endscope"), (".proc inner", ".endproc")]:
                    out[f"{cpu}_{name}_{generated}_{directive}"] = f'.setcpu "{cpu}"\n{head}:{directive}\n.byte 7\n{close}\n'
    out.update({
        "scope_address": "mark:.scope inner\n.byte 7\n.endscope\n.word mark\n",
        "proc_both_addresses": "mark:.proc inner\n.byte 7\n.endproc\n.word mark,inner\n",
        "generated_scope_address": '.ident("mark"):.scope inner\n.byte 7\n.endscope\n.word mark\n',
        "nested_scope": '.scope outer\n.ident(.concat("ma","rk")):.scope inner\n.byte 7\n.endscope\n.word mark\n.endscope\n.word outer::mark\n',
        "nested_proc": '.scope outer\nmark:.proc inner\n.byte 7\n.endproc\n.word mark,inner\n.endscope\n.word outer::mark,outer::inner\n',
        "unnamed_scope": "mark:.scope\n.byte 7\n.endscope\n.word mark\n",
        "unnamed_proc": "mark:.proc\n.byte 7\n.endscope\n.word mark\n",
        "cheap_scope": "anchor:.byte 6\n@near:.scope inner\n.byte 7\n.endscope\n.word @near\n",
        "cheap_proc": "anchor:.byte 6\n@near:.proc inner\n.byte 7\n.endproc\n.word @near\n",
        "anonymous_scope": ":.scope inner\n.byte 7\n.endscope\n.word :-\n",
        "anonymous_proc": ":.proc inner\n.byte 7\n.endproc\n.word :-\n",
        "inactive_unknown": '.if 0\n.ident(.sprintf("%bad",0)):.scope inner\n.byte 7\n.endscope\n.endif\n.byte 8\n',
        "zero_repeat": '.repeat 0\n.ident(.sprintf("%bad",0)):.proc inner\n.byte 7\n.endproc\n.endrepeat\n.byte 8\n',
        "iteration_names": '.repeat 2,I\n.ident(.sprintf("row%d",I)):.scope\n.byte I\n.endscope\n.endrepeat\n.word row0,row1\n',
        "macro_context": '.macro emit\n.ident("mark"):.scope inner\n.byte 7\n.endscope\n.endmacro\nemit\n.word mark\n',
        "include_context": '.include "body.inc"\n',
        "duplicate": "mark:.byte 6\nmark:.scope inner\n.byte 7\n.endscope\n",
        "proc_same_name": "inner:.proc inner\n.byte 7\n.endproc\n",
        "bad_generated_name": '.ident("bad;name"):.scope inner\n.byte 7\n.endscope\n',
        "bad_generated_argument": '.ident(7):.scope inner\n.byte 7\n.endscope\n',
        "static_scope_closer": ".scope inner\n.byte 7\ntail:.endscope\n.word inner::tail\n",
        "generated_scope_closer": '.scope inner\n.byte 7\n.ident("tail"):.endscope\n.word inner::tail\n',
        "static_proc_closer": ".proc inner\n.byte 7\ntail:.endproc\n.word inner::tail\n",
        "generated_proc_closer": '.proc inner\n.byte 7\n.ident("tail"):.endproc\n.word inner::tail\n',
    })
    return out

if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location("capture", Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py")
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.FILES = FILES
    runner.main()
