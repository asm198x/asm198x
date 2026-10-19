"""Retain native numeric identity across suffix and prefix token spellings."""
import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location('capture', Path(__file__).parents[1] / '2026-10-19-ca65-scope-binding/probe.py')
assert spec is not None and spec.loader is not None
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
runner.cases = lambda: {'numeric_token_identity': '.byte .xmatch({0FFh},{$FF}),.xmatch({0ffH},{255}),.xmatch({0h},{0}),.match({0FFh},{1})\n'}
runner.main()
