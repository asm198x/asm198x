"""Expose converted values in native bytes, including decimal's high endpoint."""
import importlib.util
import json
from pathlib import Path


def cases() -> dict[str, str]:
    original = json.loads(Path(__file__).with_name('native.json').read_text())
    numbers = [c['name'].removeprefix('constant_') for c in original['cases'] if c['name'].startswith('constant_')]
    return {cpu + '_' + number: f'.setcpu "{cpu}"\n.dword ({number} & $FFFFFFFF)\n'
            for cpu in ['6502', '65816', 'huc6280'] for number in numbers}


if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('capture', Path(__file__).parents[1] / '2026-10-19-ca65-scope-binding/probe.py')
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
