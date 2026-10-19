"""Retain native literal boundaries independently of final expression size."""
import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    numbers = [
        '2147483647', '2147483648', '4294967295', '4294967296',
        '4294967299', '4294967300', '9223372036854775807',
        '$FFFFFFFF', '$100000000', '$0FFFFFFFF',
        '0FFFFFFFFh', '100000000h', '0FFFFFFFFH',
        '%' + '1' * 32, '%1' + '0' * 32, '%0' + '1' * 32,
        '0004294967299', '0004294967300',
    ]
    for number in numbers:
        for context in ['difference', 'empty_tag', 'constant', 'inactive']:
            if context == 'difference':
                source = f'.byte <({number}-{number})\n'
            elif context == 'empty_tag':
                source = '.struct Empty\n.endstruct\n.struct Record\n' + f'field .tag Empty {number}\n.endstruct\n.byte 7\n'
            elif context == 'constant':
                source = f'VALUE={number}\n.byte 7\n'
            else:
                source = f'.if 0\n.byte <{number}\n.endif\n.byte 7\n'
            out[f'{context}_{number}'] = source
    return out


if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('capture', Path(__file__).parents[1] / '2026-10-19-ca65-scope-binding/probe.py')
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
