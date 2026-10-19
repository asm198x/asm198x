"""Measure native numeric admission around end and captured token sources."""
import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    overflow = '.byte <$100000000\n'
    return {
        'after_end': '.byte 7\n.end\n' + overflow,
        'on_end_line': '.byte 7\n.end $100000000\n',
        'active_condition_end': '.if 1\n.end\n' + overflow + '.endif\n',
        'active_condition_end_else': '.if 1\n.end\n.else\n' + overflow + '.endif\n',
        'inactive_end': '.if 0\n.end\n.endif\n' + overflow,
        'zero_repeat_end_body': '.repeat 0\n.end\n' + overflow + '.endrepeat\n.byte 7\n',
        'one_repeat_end_body': '.repeat 1\n.end\n' + overflow + '.endrepeat\n.byte 7\n',
        'one_repeat_end_after': '.repeat 1\n.end\n.endrepeat\n' + overflow,
        'one_repeat_end_end_after': '.repeat 1\n.end\n.endrepeat\n.end\n' + overflow,
        'macro_end_unused': '.macro unused\n.end\n' + overflow + '.endmacro\n.byte 7\n',
        'macro_end_called': '.macro stop\n.end\n.endmacro\nstop\n' + overflow,
        'macro_end_after': '.macro stop\n.end\n.endmacro\n.end\n' + overflow,
        'conditional_closer': '.if 0\n.byte 7\n.endif $100000000\n',
        'repeat_closer': '.repeat 0\n.byte 7\n.endrepeat $100000000\n',
        'macro_closer': '.macro unused\n.byte 7\n.endmacro $100000000\n.byte 7\n',
        'unused_macro_nested_condition': '.macro unused\n.if 1\n.end\n' + overflow + '.endif\n.endmacro\n.byte 7\n',
        'skipped_macro_end': '.if 0\n.macro unused\n.end\n' + overflow + '.endmacro\n.endif\n.byte 7\n',
    }


if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('capture', Path(__file__).parents[1] / '2026-10-19-ca65-scope-binding/probe.py')
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
