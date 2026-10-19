"""Retain the native global absolute cursor across physical segment switches."""
import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    for segment in ['RODATA', 'BSS', 'ZEROPAGE']:
        for size in [0, 1, 3]:
            for mode in ['keep', 'org', 'reloc']:
                change = '.org $3000\n' if mode == 'org' else '.reloc\n' if mode == 'reloc' else ''
                source = '.org $2000\nfirst:.byte 1\n' + f'.segment "{segment}"\n' + change + f'second:.res {size}\n.segment "CODE"\n.word first,second,*\n'
                out[f'{segment}_{size}_{mode}'] = source
            out[f'{segment}_{size}_align'] = '.org $2000\nfirst:.byte 1\n' + f'.segment "{segment}"\n.res {size}\n.align 4\nsecond:\n.segment "CODE"\n.word first,second,*\n'
    for directive in ['byte', 'word', 'dbyt', 'dword']:
        values = '<*,<*' if directive == 'byte' else '*,*'
        out[f'data_{directive}'] = f'.org $2000\n.segment "RODATA"\n.{directive} {values}\n.segment "CODE"\n.word *\n'
    out.update({
        'push_pop': '.org $2000\nfirst:.byte 1\n.pushseg\n.segment "RODATA"\n.res 3\n.popseg\n.word first,*\n',
        'include': '.org $2000\nfirst:.byte 1\n.include "counter.inc"\n.word first,second,*\n',
        'macro': '.macro emit\n.segment "RODATA"\nsecond:.byte 2\n.segment "CODE"\n.endmacro\n.org $2000\nfirst:.byte 1\nemit\n.word first,second,*\n',
        'repeat': '.org $2000\n.repeat 2,I\n.segment "RODATA"\n.byte I\n.segment "CODE"\n.word *\n.endrepeat\n',
        'alias_definition_pc': '.org $2000\n.byte 1\n.segment "RODATA"\nALIAS=*\n.byte 2\n.segment "CODE"\n.word ALIAS,*\n',
        'absolute_bit_branch': '.setcpu "4510"\n.org $2000\n.segment "RODATA"\n.byte 1\n.segment "CODE"\nbbr0 $20,$2004\n',
        'relative_bit_branch': '.setcpu "4510"\n.org $2000\n.segment "RODATA"\n.reloc\n.segment "CODE"\nbbr0 $20,$2000\n',
    })
    return out


if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('capture', Path(__file__).parents[1] / '2026-10-19-ca65-scope-binding/probe.py')
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.CONFIG = 'MEMORY { MAIN:start=$1000,size=$7000,file=%O; RAM:start=$20,size=$100,file=""; }\nSEGMENTS { CODE:load=MAIN,type=ro; RODATA:load=MAIN,type=ro; ZEROPAGE:load=RAM,type=zp; BSS:load=RAM,type=bss; }\n'
    runner.FILES = {'counter.inc': '.segment "RODATA"\nsecond:.byte 2\n.segment "CODE"\n'}
    runner.cases = cases
    runner.main()
