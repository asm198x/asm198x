"""Retain native logical addresses independently of sidecar coordinates."""
import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    for origin in ['$20', '$1000', '$2000']:
        for namespace in ['global', 'cheap', 'scope']:
            label = '@item' if namespace == 'cheap' else 'item'
            source = f'.org {origin}\n' + ('head:\n' if namespace == 'cheap' else '') + f'{label}:.byte 7\n.word {label}\n'
            if namespace == 'scope':
                source = '.scope inner\n' + source + '.endscope\n'
            out[f'{origin}_{namespace}'] = source
    out.update({
        'metadata_only': '.org $2000\nitem:\n.word item\n',
        'mode_label': '.org $2000\nitem:.reloc\n.word item\n',
        'multiple_origins': '.org $2000\nfirst:.byte 7\n.org $3000\nsecond:.byte 8\n.word first,second\n',
        'segment_change': '.org $2000\nfirst:.byte 1\n.segment "RODATA"\nsecond:.byte 2\n.segment "CODE"\n.word first,second,*\n',
        'other_segment_reloc': '.org $2000\nfirst:.byte 1\n.segment "RODATA"\n.reloc\nsecond:.byte 2\n.segment "CODE"\n.word first,second,*\n',
        'each_segment_origin': '.org $2000\nfirst:.byte 1\n.segment "RODATA"\n.org $3000\nsecond:.byte 2\n.segment "CODE"\n.word first,second,*\n',
        'return_relative_segment': 'first:.byte 1\n.segment "RODATA"\n.org $3000\nsecond:.byte 2\n.segment "CODE"\n.word first,second,*\n',
    })
    return out


if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('capture', Path(__file__).parents[1] / '2026-10-19-ca65-scope-binding/probe.py')
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.CONFIG = 'MEMORY { MAIN:start=$1000,size=$7000,file=%O; }\nSEGMENTS { CODE:load=MAIN,type=ro; RODATA:load=MAIN,type=ro; }\n'
    runner.cases = cases
    runner.main()
