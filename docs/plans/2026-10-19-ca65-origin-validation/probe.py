"""Retain native source availability and range controls for .org."""
import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    for value in ['-1', '0', '$FFFF', '$10000', '$FFFFFF', '$1000000', '$FFFFFFFF']:
        out[f'bound_{value}'] = f'.org {value}\n.byte 7\n'
    for expr in ['VALUE', 'VALUE+1', '<VALUE', 'VALUE-VALUE', '0&&VALUE', '1||VALUE']:
        for phase in ['known', 'forward', 'missing']:
            before = 'VALUE=$2000\n' if phase == 'known' else ''
            after = 'VALUE=$2000\n' if phase == 'forward' else ''
            out[f'{phase}_{expr}'] = before + f'.org {expr}\n' + after + '.byte 7\n'
    for mode in ['', '.org $2000\n']:
        for expr in ['*', '*+3', '*-*', 'item', 'item+3', 'end-item', 'ALIAS', 'ALIAS+1']:
            out[f'pc_{bool(mode)}_{expr}'] = mode + 'item:.byte 0\nALIAS=*\nend:\n' + f'.org {expr}\n.byte 7\n'
    out.update({
        'resolved_alias_chain': 'FIRST=SECOND\nSECOND=$2000\n.org FIRST\n.byte 7\n',
        'unresolved_alias_chain': 'FIRST=SECOND\n.org FIRST\nSECOND=$2000\n.byte 7\n',
        'alias_definition_pc': '.org $2000\nALIAS=*\n.byte 0\n.org ALIAS\n.word *\n',
        'relative_label_difference': 'start:.byte 0,0\nend:\n.org end-start\n.word *\n',
        'cross_segment_difference': 'start:.byte 0\n.segment "RODATA"\nend:.byte 0\n.org end-start\n.byte 7\n',
        'scope_fallback': 'VALUE=$2000\n.scope inner\n.org VALUE\n.byte 7\n.endscope\n',
        'scope_later_shadow': 'VALUE=$2000\n.scope inner\n.org VALUE\nVALUE=$3000\n.byte 7\n.endscope\n',
        'scope_qualified': '.scope inner\nVALUE=$2000\n.endscope\n.org inner::VALUE\n.byte 7\n',
        'cycle': 'FIRST=SECOND\nSECOND=FIRST\n.org FIRST\n.byte 7\n',
        'lazy_known': 'FIRST=2\n.org 1||FIRST\n.byte 7\n',
        'include_known': '.include "origin.inc"\n.org VALUE\n.byte 7\n',
        'include_forward': '.org VALUE\n.include "origin.inc"\n.byte 7\n',
        'macro': '.macro place, address\n.org address\n.byte 7\n.endmacro\nplace $2000\n',
        'repeat': '.repeat 2,I\n.org $2000+I\n.byte 7\n.endrepeat\n',
        'inactive': '.if 0\n.org MISSING\n.endif\n.byte 7\n',
        'reloc_return': '.org $2000\n.byte 0\n.reloc\n.org *\n.byte 7\n',
    })
    return out


if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('capture', Path(__file__).parents[1] / '2026-10-19-ca65-scope-binding/probe.py')
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.CONFIG = 'MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { CODE:load=MAIN,type=ro; RODATA:load=MAIN,type=ro; }\n'
    runner.FILES = {'origin.inc': 'VALUE=$2000\n'}
    runner.cases = cases
    runner.main()
