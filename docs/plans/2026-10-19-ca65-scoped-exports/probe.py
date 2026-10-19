"""Capture visibility declaration binding and allocation in named scopes."""

import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    for word in ["export", "exportzp", "import", "importzp", "global", "globalzp"]:
        declaration = f".{word} RED\n"
        for value, definition in [
            ("constant", "RED=2\n"),
            ("label", "RED:.byte 2\n"),
            ("alias", "RED=BLUE\nBLUE=2\n"),
        ]:
            for before in [False, True]:
                body = declaration + definition if before else definition + declaration
                for scope in [False, True]:
                    wrapped = ".scope inner\n" + body + ".endscope\n" if scope else body
                    out[f"{word}_{value}_{before}_{scope}"] = wrapped + ".byte 7\n"
        for before in [False, True]:
            for use in [False, True]:
                for local in [False, True]:
                    definition = "RED=3\n" if local else ""
                    body = (
                        declaration + definition if before else definition + declaration
                    )
                    body += ".byte RED\n" if use else ".byte 7\n"
                    out[f"{word}_fallback_{before}_{use}_{local}"] = (
                        "RED=2\n.scope inner\n" + body + ".endscope\n"
                    )
        for name, source in {
            "nested": ".scope inner\nRED=2\n.scope nested\n"
            + declaration
            + ".byte RED\n.endscope\n.endscope\n",
            "late_parent": ".scope inner\n.scope nested\n"
            + declaration
            + ".byte RED\n.endscope\nRED=2\n.endscope\n",
            "qualified": ".scope inner\nRED=2\n.endscope\n"
            + f".{word} inner::RED\n.byte 7\n",
            "root_qualified": "RED=2\n" + f".{word} ::RED\n.byte 7\n",
            "queries_before_definition": ".scope inner\n"
            + declaration
            + ".byte .defined(RED),.referenced(RED),.const(RED)\nRED=2\n.byte .defined(RED),.referenced(RED),.const(RED)\n.endscope\n",
            "queries_with_outer": "RED=2\n.scope inner\n"
            + declaration
            + ".byte .defined(RED),.referenced(RED),.const(RED)\nRED=3\n.endscope\n",
            "empty_queries": ".scope inner\n"
            + declaration
            + ".byte .defined(RED),.referenced(RED),.const(RED)\n.endscope\n",
            "cheap": "head:\n@RED:.byte 2\n" + f".{word} @RED\n.byte 7\n",
            "list": ".scope inner\nRED=2\nBLUE=3\n"
            + f".{word} RED,BLUE\n.endscope\n.byte 7\n",
            "repeat_declaration": ".scope inner\nRED=2\n"
            + declaration * 2
            + ".endscope\n.byte 7\n",
            "missing": ".scope inner\n" + declaration + ".endscope\n.byte 7\n",
            "used_missing": ".scope inner\n" + declaration + ".byte RED\n.endscope\n",
        }.items():
            out[f"{word}_{name}"] = source
        for expr in ["2", "BLUE", "*", "<BLUE"]:
            source = (
                f".scope inner\n.{word} RED := {expr}\nBLUE=2\n.word RED\n.endscope\n"
            )
            out[f"{word}_defines_{expr}"] = source
    out["two_exports"] = (
        ".scope first\nRED=2\n.export RED\n.endscope\n.scope second\nRED=3\n.export RED\n.endscope\n.byte 7\n"
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
    runner.main()
