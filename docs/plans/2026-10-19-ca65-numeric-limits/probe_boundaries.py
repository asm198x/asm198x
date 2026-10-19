"""Retain native token boundaries independently of selected expression parsing."""
import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    tokens = [
        '0xFF', '123G', '123ABC', '0FFFFFFFFh', '100000000h',
        '$100000000G', '$FFFFFFFFG', '$', '%',
        '%100000000000000000000000000000000', '%11111111111111111111111111111111',
        '%102', '123_4294967300', 'name4294967300', 'name$100000000',
        '@name4294967300', '4294967300G', '4294967299G', '4294967300_',
        '123456789012345678901234567890', '00000000000000000000000000000001',
        '0123h', '$0_FFFFFFFF', '0FFH', '0FFhG', '0ff_4294967300',
        "'4294967300'", '"4294967300"', '$FFFFFFFFh',
        '0FFFFFFFFh4294967300', '0FFFFFFFFh_', '0FFFFFFFFHfoo',
        '42949672950h', '4294967299h', '4294967300h', '$100000000;ignored',
    ]
    out = {}
    for index, token in enumerate(tokens):
        out[f'active_{index}'] = f'.byte <{token}\n'
        out[f'inactive_{index}'] = f'.if 0\n.byte <{token}\n.endif\n.byte 7\n'
    return out


if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('capture', Path(__file__).parents[1] / '2026-10-19-ca65-scope-binding/probe.py')
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
