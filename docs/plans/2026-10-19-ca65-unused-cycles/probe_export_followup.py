import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "capture", Path("docs/plans/2026-10-19-ca65-scope-binding/probe.py")
)
assert spec and spec.loader
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
m.cases = lambda: {
    "scoped_export": ".scope inner\nRED=2\n.export RED\n.endscope\n.byte 7\n",
    "root_export": "RED=2\n.export RED\n.byte 7\n",
    "qualified_export": ".scope inner\nRED=2\n.endscope\n.export inner::RED\n.byte 7\n",
    "scoped_export_root_fallback": "RED=2\n.scope inner\n.export RED\n.endscope\n.byte 7\n",
}
m.main()
