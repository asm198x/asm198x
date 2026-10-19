import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "capture", Path("docs/plans/2026-10-19-ca65-scope-binding/probe.py")
)
assert spec and spec.loader
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
m.cases = lambda: {
    f"{head}_{expr}": ".scope inner\nRED=<RED\n.endscope\n"
    + f"{head} {expr}\n.byte 7\n.else\n.byte 9\n.endif\n"
    for head in [".ifconst", ".ifnconst"]
    for expr in ["inner::RED", "0&&inner::RED", "1||inner::RED"]
}
m.main()
