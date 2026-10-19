"""Retain independent origin-validation gaps without changing the counter cohort."""

import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "capture", Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
)
assert spec is not None and spec.loader is not None
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
runner.cases = lambda: {
    "forward_origin": ".org VALUE\nVALUE=$2000\n.byte 7\n",
    "relocatable_pc_origin": ".org *+3\n.byte 7\n",
    "absolute_pc_origin": ".org $2000\n.org *+3\n.byte 7\n",
    "outside_24bit_origin": ".org $1000000\n.byte 7\n",
    "maximum_24bit_origin": ".org $FFFFFF\n.byte 7\n",
}
runner.main()
