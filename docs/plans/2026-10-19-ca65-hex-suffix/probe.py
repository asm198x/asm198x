"""Retain native active h/H token grammar and quoted baseline controls."""
import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    for number in ['0h', '0H', '0FFh', '0ffH', '01234h', '0FFFFFFFFh', '0000FFFFFFFFH', '100000000h', '0FFFFFFFFFh']:
        for context in ['difference', 'constant']:
            out[f'{context}_{number}'] = f'.byte <({number}-{number})\n' if context == 'difference' else f'VALUE={number}\n.byte <VALUE,>VALUE\n'
    out.update({
        'word': '.word 01234h\n',
        'dword': '.dword 0FFFFFFFFh\n',
        'reservation': '.res 2h,0A5h\n',
        'macro': '.macro emit operand\n.byte operand\n.endmacro\nemit 0FFh\n',
        'define': '.define VALUE 0FFh\n.byte VALUE\n',
        'include': '.include "number.inc"\n',
        'quoted': '.byte "0FFh" ; 100000000h\n',
        'decimal_prefix_baseline': '.byte 17,$12,%1010\n',
        'invalid_digits': '.byte 012Gh\n',
        'bare_identifier': '.byte AAh\n',
        'lazy_overflow': '.byte 1||100000000h\n',
    })
    return out


if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('capture', Path(__file__).parents[1] / '2026-10-19-ca65-scope-binding/probe.py')
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.FILES = {'number.inc': 'VALUE=0FFh\n.byte VALUE\n'}
    runner.cases = cases
    runner.main()
