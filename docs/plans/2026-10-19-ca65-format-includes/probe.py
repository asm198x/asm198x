"""Capture no-resolution include and binary-window formatter boundaries."""
import importlib.util
from pathlib import Path

FILES = {'payload.inc': '.byte $4c\n', 'bytes.bin': '\x00\x11\x22\x33', 'semi;colon.inc': '.byte 5\n', 'comma,name.inc': '.byte 6\n', 'space name.inc': '.byte 9\n'}


def cases() -> dict[str, str]:
    plain = {
        'include_literal': '.include "payload.inc"\n',
        'include_computed': '.include .sprintf("%s","payload.inc")\n',
        'incbin_literal': '.incbin "bytes.bin"\n',
        'incbin_computed': '.incbin .sprintf("%s","bytes.bin"),1,2\n',
        'constant_window': 'OFFSET=1\nCOUNT=2\n.incbin "bytes.bin",OFFSET,COUNT\n',
        'expression_window': '.incbin "bytes.bin",1+1,1\n',
        'literal_label': 'here: .include "payload.inc"\n.word here\n',
        'computed_label': 'here: .include .sprintf("%s","payload.inc") ; inline\n.word here\n',
        'binary_label': 'here: .incbin .sprintf("%s","bytes.bin"),1,2 ; inline\n.word here\n',
        'semicolon_name': '.include "semi;colon.inc" ; actual comment\n',
        'comma_name': '.include "comma,name.inc"\n',
        'space_name': '.include "space name.inc"\n',
        'missing_computed': '.include .sprintf("%s","missing.inc")\n',
        'forward_window': '.incbin "bytes.bin",OFFSET,SIZE\nOFFSET=1\nSIZE=2\n',
        'negative_window': '.incbin "bytes.bin",-1,1\n',
        'number_include': '.include 3\n',
        'empty_window': '.incbin "bytes.bin",1,,2\n',
        'undefined_window': '.incbin "bytes.bin",MISSING,1\n',
    }
    out = dict(plain)
    for cpu in ['65816', 'huc6280']:
        for name, source in plain.items():
            out[f'{cpu}_{name}'] = f'.setcpu "{cpu}"\n.org $1000\n' + source
    for name, operand in {
        'number_include': '.include 3',
        'computed_missing': '.include .sprintf("%s","missing.inc")',
        'missing_literal': '.include "missing.inc"',
        'undefined_window': '.incbin "bytes.bin",MISSING,1',
        'forward_window': '.incbin "bytes.bin",OFFSET,SIZE',
        'divide_zero': '.incbin "bytes.bin",1/0,1',
        'negative_window': '.incbin "bytes.bin",-1,1',
        'empty_window': '.incbin "bytes.bin",1,,2',
    }.items():
        out['skipped_' + name] = f'.if 0\n{operand}\n.endif\nOFFSET=1\nSIZE=2\n.byte 7\n'
    out['anonymous_include_label'] = '.word :+\n: .include "payload.inc"\n'
    return out


if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('capture', Path(__file__).parents[1] / '2026-10-19-ca65-scope-binding/probe.py')
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.FILES = FILES
    runner.main()
