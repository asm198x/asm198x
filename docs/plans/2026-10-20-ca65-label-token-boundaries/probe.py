"""Retain native label-token identity across delimiters and source capture."""

import importlib.util
from pathlib import Path

FILES = {"label.inc": "F : .byte 7\n.word F\n"}


def cases() -> dict[str, str]:
    out = {}
    names = [
        "mark",
        "A",
        "a",
        "X",
        "Y",
        "F",
        "f",
        "Z",
        "z",
        "S",
        "SP",
        "LDA",
        "INA",
        "BRU",
    ]
    for cpu in ["6502", "4510"]:
        prefix = f'.setcpu "{cpu}"\n'
        for name in names:
            for gap_name, gap in [("attached", ""), ("space", " "), ("tab", "\t")]:
                for origin in ["literal", "ident", "define", "argument"]:
                    head = (
                        f'.ident("{name}")'
                        if origin == "ident"
                        else "NAME"
                        if origin == "define"
                        else name
                    )
                    setup = f".define NAME {name}\n" if origin == "define" else ""
                    body = setup + f'{head}{gap}:.byte 7\n.word .ident("{name}")\n'
                    if origin == "argument":
                        body = f'.macro emit arg\narg{gap}:.byte 7\n.endmacro\nemit {name}\n.word .ident("{name}")\n'
                    out[f"{cpu}_{name}_{gap_name}_{origin}"] = prefix + body
    for before, after in [("6502", "4510"), ("4510", "6502")]:
        for name in ["S", "SP", "Z", "F"]:
            prefix = f'.setcpu "{before}"\n'
            switch = f'.setcpu "{after}"\n'
            tail = f'.word .ident("{name}")\n'
            out[f"cpu_{before}_{after}_{name}_define"] = (
                prefix + f".define NAME {name}\n" + switch + "NAME:.byte 7\n" + tail
            )
            out[f"cpu_{before}_{after}_{name}_macro_literal"] = (
                prefix
                + f".macro emit\n{name}:.byte 7\n.endmacro\n"
                + switch
                + "emit\n"
                + tail
            )
            out[f"cpu_{before}_{after}_{name}_macro_argument"] = (
                prefix
                + ".macro emit arg\narg:.byte 7\n.endmacro\n"
                + switch
                + f"emit {name}\n"
                + tail
            )
            out[f"cpu_{before}_{after}_{name}_repeat"] = (
                prefix + ".repeat 1\n" + switch + f"{name}:.byte 7\n.endrepeat\n" + tail
            )
            for gap_name, gap in [("attached", ""), ("space", " ")]:
                out[f"cpu_{before}_{after}_{name}_{gap_name}_captured_call"] = (
                    prefix
                    + ".macro emit\n.byte 7\n.endmacro\n"
                    + f".macro outer\n{name}{gap}:emit\n.endmacro\n"
                    + switch
                    + "outer\n"
                    + tail
                )
    out.update(
        {
            "cheap_space": "entry:.byte 6\n@cheap : .byte 7\n.word @cheap\n",
            "cheap_tab": "entry:.byte 6\n@cheap\t: .byte 7\n.word @cheap\n",
            "cheap_digit_space": "entry:.byte 6\n@1 : .byte 7\n.word @1\n",
            "cheap_dotted_space": "entry:.byte 6\n@bad.dot : .byte 7\n",
            "scoped_prefix_space": ".scope inner\nF : .byte 7\n.word F\n.endscope\n",
            "included_prefix_space": '.include "label.inc"\n',
            "skipped_register": ".if 0\nA : .byte 7\n.endif\n.byte 8\n",
            "unused_register": ".macro unused\nA : .byte 7\n.endmacro\n.byte 8\n",
            "zero_register": ".repeat 0\nA : .byte 7\n.endrepeat\n.byte 8\n",
            "selected_register": ".if 1\nA : .byte 7\n.endif\n",
            "quoted_colon": '.byte "F :", "A:", 7\n',
            "operand_prefix": "lda F:$1234\n",
            "operand_separated_prefix": "lda F : $1234\n",
            "anonymous": ": .byte 7\n.word :-\n",
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
