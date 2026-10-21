"""Retain native external allocation, source classification and final values."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent
FILES: dict[str, str] = {"read2.inc": ".byte 7\n"}


def cases() -> dict[str, str]:
    bodies = {}
    for word in ["import", "importzp", "global", "globalzp"]:
        declaration = f".{word} RED\n"
        scoped = ".scope inner\n" + declaration
        bodies[word + "_unused_outer"] = (
            "RED=2\n" + scoped + ".byte .defined(RED),.ref(RED)\n.endscope\n"
        )
        bodies[word + "_defined"] = (
            scoped + ".if .defined(RED)\n.byte 7\n.else\n.byte 9\n.endif\n.endscope\n"
        )
        bodies[word + "_const_noexport"] = (
            scoped + ".if .const(RED)\n.byte 7\n.else\n.byte 9\n.endif\n.endscope\n"
        )
        for before in [True, False]:
            definition = "RED=3\n"
            body = definition + declaration if before else declaration + definition
            bodies[word + f"_definition_{before}"] = (
                ".scope inner\n" + body + ".word RED\n.endscope\n"
            )
        operations = {
            "byte": ".byte RED\n",
            "word": ".word RED\n",
            "instruction": "lda RED\n",
            "low_byte": ".byte <RED\n",
            "cancel": ".word RED-RED\n",
            "zero": ".byte RED*0\n",
            "conditional": ".if .const(RED)\n.byte 7\n.else\n.byte 9\n.endif\n.byte .ref(RED)\n",
        }
        for name, operation in operations.items():
            for late in [False, True]:
                export = "RED=2\n.export RED\n"
                source = scoped + operation + ".endscope\n"
                bodies[word + "_export_" + name + f"_{late}"] = (
                    source + export if late else export + source
                )
        for name, expression in {
            "bare": "RED",
            "cancel": "RED-RED",
            "zero": "RED*0",
            "low": "<RED",
            "add": "RED+0",
        }.items():
            bodies[word + "_classification_" + name] = (
                "RED=2\n.export RED\n"
                + scoped
                + f".if .const({expression})\n.byte 7\n.else\n.byte 9\n.endif\n"
                + ".byte .defined(RED),.ref(RED)\n.word RED\n.endscope\n"
            )
        for second in ["import", "importzp", "global", "export"]:
            bodies[word + "_mixed_" + second] = (
                scoped + f".{second} RED\nRED=2\n.word RED\n.endscope\n"
            )
    for word in ["import", "importzp", "global", "globalzp", "export", "exportzp"]:
        for order, body in {
            "forward_instruction": f".{word} RED\nlda RED\nRED=2\n",
            "defined_instruction": f".{word} RED\nRED=2\nlda RED\n",
            "declaration_last": f"RED=2\n.{word} RED\nlda RED\n",
        }.items():
            bodies[word + "_local_width_" + order] = (
                ".scope inner\n" + body + ".endscope\n"
            )
    for word in ["import", "importzp", "global", "globalzp"]:
        for operation in ["byte", "lda"]:
            for name, expression in {
                "add": "RED+1",
                "multiply": "RED*2",
                "mask": "RED&255",
                "or": "RED|0",
                "shift": "RED>>1",
            }.items():
                op = ".byte" if operation == "byte" else "lda"
                bodies[word + "_arithmetic_" + operation + "_" + name] = (
                    "RED=2\n.export RED\n.scope inner\n"
                    + f".{word} RED\n{op} {expression}\n.endscope\n"
                )
    pairs = [
        ("globalzp", "global"),
        ("global", "globalzp"),
        ("exportzp", "global"),
        ("global", "exportzp"),
        ("export", "globalzp"),
        ("exportzp", "export"),
        ("importzp", "global"),
        ("globalzp", "import"),
        ("import", "globalzp"),
        ("global", "importzp"),
        ("import", "export"),
        ("export", "import"),
        ("global", "export"),
        ("export", "global"),
    ]
    for first, second in pairs:
        for defined in [False, True]:
            start = (
                ".scope inner\nRED=2\n"
                if defined
                else "RED=2\n.export RED\n.scope inner\n"
            )
            bodies[f"pair_{first}_{second}_{defined}"] = (
                start
                + f".{first} RED\n.{second} RED\n"
                + ".byte .defined(RED),.ref(RED)\n"
                + ".if .const(RED)\n.byte 7\n.else\n.byte 9\n.endif\n"
                + ".word RED\nlda RED\n.endscope\n"
            )
    for word in ["global", "globalzp"]:
        for name, expression in {
            "bare": "RED",
            "zero": "RED*0",
            "cancel": "RED-RED",
        }.items():
            bodies[word + "_forward_byte_" + name] = (
                f".scope inner\n.{word} RED\n.byte {expression}\nRED=2\n.endscope\n"
            )
    for word in ["import", "importzp", "global", "globalzp", "none"]:
        for name, expression in {
            "zero": "RED*0",
            "cancel": "RED-RED",
            "and": "0&&RED",
            "or": "1||RED",
            "defined": ".defined(RED)",
            "referenced": ".ref(RED)",
        }.items():
            declaration = "" if word == "none" else f".{word} RED\n"
            bodies[word + "_query_flags_" + name] = (
                "RED=2\n.export RED\n.scope inner\n"
                + declaration
                + f".if .const({expression})\n.byte 7\n.else\n.byte 9\n.endif\n"
                + ".byte .ref(RED)\n.endscope\n"
            )
    for word in ["import", "importzp", "global", "globalzp"]:
        for name, operation in {
            "string_data": '.byte .sprintf("%d",RED)\n',
            "string_out": '.out .sprintf("%d",RED)\n.byte 7\n',
            "string_include": '.include .sprintf("read%d.inc",RED)\n',
            "org": ".org RED\n.byte 7\n",
            "assert": '.assert RED=2,error,"value"\n.byte 7\n',
            "immediate": "lda #RED\n",
            "low_immediate": "lda #<RED\n",
            "forced_zp": "lda z:RED\n",
            "repeat": ".repeat RED\n.byte 7\n.endrepeat\n",
            "if_cancel": ".if RED-RED\n.byte 7\n.else\n.byte 9\n.endif\n",
            "string_cancel": '.byte .sprintf("%d",RED-RED),.ref(RED)\n',
        }.items():
            bodies[word + "_consumer_" + name] = (
                f"RED=2\n.export RED\n.scope inner\n.{word} RED\n"
                + operation
                + ".endscope\n"
            )
    assert len(bodies) == 278
    out = {}
    for name, body in bodies.items():
        for context in ["direct", "included", "macro", "inactive", "unused"]:
            key = name + "_" + context
            if context == "included":
                FILES[key + ".inc"] = body
                source = f'.include "{key}.inc"\n'
            elif context in ["macro", "unused"]:
                source = (
                    ".macro run\n"
                    + body
                    + ".endmacro\n"
                    + ("run\n" if context == "macro" else ".byte 9\n")
                )
            elif context == "inactive":
                source = ".if 0\n" + body + ".endif\n.byte 9\n"
            else:
                source = body
            out[key] = source
    assert len(out) == 1390
    return out


if __name__ == "__main__":
    spec = importlib.util.spec_from_file_location(
        "capture", ROOT.parent / "2026-10-19-ca65-scope-binding/probe.py"
    )
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    prepared = cases()
    runner.FILES = FILES
    runner.cases = lambda: prepared
    runner.main()
