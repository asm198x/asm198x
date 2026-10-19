"""Measure source-time cancellation before designing origin validation."""
import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    for absolute in [False, True]:
        for expression in [
            '0*item', 'item*0', 'item&0', 'item|0', 'item^item',
            '(end-item)*2', '<(end-item)', '-(end-item)', '*-*',
            'item-item', 'end-item', '(item+1)-item',
            'ALIAS-item', 'ALIAS-ALIAS', '1||item', '0&&item',
        ]:
            prefix = '.org $2000\n' if absolute else ''
            out[f'{absolute}_{expression}'] = prefix + 'item:.byte 0\nend:\nALIAS=item\n' + f'.org {expression}\n.word *\n'
    return out


if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('capture', Path(__file__).parents[1] / '2026-10-19-ca65-scope-binding/probe.py')
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
