"""Measure repeat-counter name admission without changing the qualified source."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
prepared = {}
for name in ["A", "X", "Y", "Z", "S", "INDEX"]:
    for form, word in [("plain", name), ("ident", f'.ident("{name}")')]:
        prepared[name + "_" + form] = f".repeat 1,{word}\n.byte 7\n.endrepeat\n"
for name in ["A", "X", "Y", "Z", "S", "INDEX"]:
    prepared[name + "_inactive"] = (
        f".if 0\n.repeat 1,{name}\n.byte 7\n.endrepeat\n.endif\n.byte 9\n"
    )
spec = importlib.util.spec_from_file_location(
    "capture", ROOT / "docs/plans/2026-10-19-ca65-scope-binding/probe.py"
)
assert spec and spec.loader
c = importlib.util.module_from_spec(spec)
spec.loader.exec_module(c)
c.cases = lambda: prepared
c.FILES = {}
c.main()
