"""Measure repeat termination, capture and deferred character-input boundaries."""
import importlib.util
import json
from pathlib import Path

FILES = {'oversized.inc': '.byte <$100000000\n', 'ends.inc': '.end\n.byte <$100000000\n', 'prefix.inc': '.byte 9\n'}


def cases() -> dict[str, str]:
    old = json.loads((Path(__file__).parents[1] / '2026-10-19-ca65-numeric-limits/repeat-end-extended.json').read_text())
    out = {}
    seen = set()
    for case in old['cases']:
        if case['source'] not in seen:
            out[case['name']] = case['source']
            seen.add(case['source'])
    out.update({
        'deferred_oversized_include': '.repeat 2\n.include "oversized.inc"\n.end\n.endrepeat\n.byte 8\n',
        'missing_include_before_end': '.repeat 2\n.include "missing.inc"\n.end\n.endrepeat\n.byte 8\n',
        'missing_include_after_end': '.repeat 2\n.end\n.include "missing.inc"\n.endrepeat\n.byte 8\n',
        'zero_repeat_missing_include': '.repeat 0\n.include "missing.inc"\n.end\n.endrepeat\n.byte 8\n',
        'oversized_capture_after_end': '.repeat 1\n.end\n.byte <$100000000\n.endrepeat\n.byte 8\n',
        'zero_oversized_capture_after_end': '.repeat 0\n.end\n.byte <$100000000\n.endrepeat\n.byte 8\n',
        'conditional_false_end': '.repeat 2\n.if 0\n.end\n.else\n.byte 7\n.endif\n.endrepeat\n.byte 8\n',
        'macro_end_inside_repeat': '.macro stop\n.end\n.endmacro\n.repeat 2\n.byte 7\nstop\n.endrepeat\n.byte 8\n',
        'include_ends_reader': '.repeat 1\n.include "ends.inc"\n.byte 7\n.endrepeat\n.byte 8\n',
        'ordinary_counter': '.repeat 2,I\n.byte I\n.endrepeat\n.byte 8\n',
        'ordinary_zero': '.repeat 0\n.byte 7\n.endrepeat\n.byte 8\n',
        'ordinary_deferred_include': '.repeat 2\n.include "prefix.inc"\n.byte 7\n.endrepeat\n.byte 8\n',
    })
    assert len(out) == 22
    return out


if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('capture', Path(__file__).parents[1] / '2026-10-19-ca65-scope-binding/probe.py')
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.FILES = FILES
    runner.main()
