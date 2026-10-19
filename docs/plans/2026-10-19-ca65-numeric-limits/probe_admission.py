"""Retain native lexical admission behavior around skipped source and text."""
import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    for number in ['4294967296', '4294967300', '$100000000']:
        sources = {
            'quoted': f'.byte "{number}"\n',
            'comment': f'; {number}\n.byte 7\n',
            'unused_macro': f'.macro unused\n.byte <{number}\n.endmacro\n.byte 7\n',
            'called_macro': f'.macro emit\n.byte <{number}\n.endmacro\nemit\n',
            'macro_argument': f'.macro emit operand\n.byte <operand\n.endmacro\nemit {number}\n',
            'unused_define': f'.define UNUSED {number}\n.byte 7\n',
            'used_define': f'.define VALUE {number}\n.byte <VALUE\n',
            'zero_repeat': f'.repeat 0\n.byte <{number}\n.endrepeat\n.byte 7\n',
            'one_repeat': f'.repeat 1\n.byte <{number}\n.endrepeat\n',
            'after_end': f'.byte 7\n.end\n.byte <{number}\n',
            'skipped_define': f'.if 0\n.define VALUE {number}\n.endif\n.byte 7\n',
            'nested_inactive': f'.if 0\n.if MISSING\n.byte <{number}\n.endif\n.endif\n.byte 7\n',
            'inactive_include': f'.if 0\n.include "absent-{number}.inc"\n.endif\n.byte 7\n',
            'quoted_define': f'.define VALUE "{number}"\n.byte VALUE\n',
            'cpu_65816': f'.setcpu "65816"\nVALUE={number}\n.byte 7\n',
            'cpu_65c02': f'.setcpu "65C02"\nVALUE={number}\n.byte 7\n',
        }
        for name, source in sources.items():
            out[f'{name}_{number}'] = source
    return out


if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('capture', Path(__file__).parents[1] / '2026-10-19-ca65-scope-binding/probe.py')
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
