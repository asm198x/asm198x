"""Retain native file-end tails that must stay unread during conversion."""
import importlib.util
from pathlib import Path

FILES = {'ending.inc': '.byte 7\n.end .sprintf("%d",4294967300)\n.byte <$100000000\n'}


def cases() -> dict[str, str]:
    return {
        'end_tail_decimal': '.byte 7\n.end .sprintf("%d",4294967300)\n.byte <$100000000\n',
        'end_tail_hex': '.byte 7\n.end .sprintf("%d",$100000000)\n.byte <$100000000\n',
        'end_tail_ident': '.byte 7\n.end .ident(.sprintf("X%u",4294967300))\n.byte <$100000000\n',
        'end_tail_include': '.include "ending.inc"\n.byte 9\n',
        'captured_end_decimal': '.macro finish\n.end .sprintf("%d",4294967300)\n.endmacro\nfinish\n',
        'captured_end_hex_suffix': '.macro finish\n.end .sprintf("%d",100000000h)\n.endmacro\nfinish\n',
    }


if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('capture', Path(__file__).parents[1] / '2026-10-19-ca65-scope-binding/probe.py')
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.FILES = FILES
    runner.main()
