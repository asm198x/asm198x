"""Retain selected numeric conversion controls under each flat CPU."""
import importlib.util
import json
from pathlib import Path


def cases() -> dict[str, str]:
    original = json.loads(Path(__file__).with_name('native.json').read_text())
    return {cpu + '_' + case['name']: f'.setcpu "{cpu}"\n' + case['source']
            for cpu in ['65816', 'huc6280'] for case in original['cases']
            if case['name'].startswith(('difference_', 'constant_'))}


if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('capture', Path(__file__).parents[1] / '2026-10-19-ca65-scope-binding/probe.py')
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
