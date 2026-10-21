"""Retain native external allocation, source classification and final values."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent
FILES: dict[str, str] = {}


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
    assert len(bodies) == 130
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
    assert len(out) == 650
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
