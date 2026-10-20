"""Retain native range/parser contexts for both flat ca65 CPU readers."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).parent
FILES: dict[str, str] = {}


def cases() -> dict[str, str]:
    out = {}
    errors = {
        "byte_range": ".byte 256",
        "word_range": ".word 65536",
        "faraddr_range": ".faraddr 16777216",
        "undefined": ".byte MISSING",
        "syntax": ".byte )",
        "negative_reserve": ".res -1",
    }
    for cpu in ["65816", "huc6280"]:
        preamble = f'.setcpu "{cpu}"\n.org $1000\n'
        for error, statement in [*errors.items(), ("valid", ".byte 7")]:
            for context in ["root", "included", "nested_root", "nested_included"]:
                nested = context.startswith("nested_")
                body = ".macro inner\n" + statement + "\n.endmacro\n"
                if nested:
                    body += ".macro outer\ninner\n.endmacro\nouter\n"
                else:
                    body += "inner\n"
                name = f"{cpu}_{error}_{context}"
                if context.endswith("included"):
                    filename = name + ".inc"
                    FILES[filename] = body
                    out[name] = preamble + f'.include "{filename}"\n'
                else:
                    out[name] = preamble + body
            if error != "valid":
                out[f"{cpu}_{error}_plain"] = preamble + statement + "\n"
                out[f"{cpu}_{error}_unused"] = (
                    preamble + ".macro unused\n" + statement + "\n.endmacro\n.byte 7\n"
                )
    assert len(out) == 80
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
