"""Measure native statement-head, scoped and cheap-label reservation."""
import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    for cpu in ['6502', '4510']:
        prefix = f'.setcpu "{cpu}"\n'
        for name in ['lda', 'ldA', 'nop', 'adc', 'INA', 'DEA', 'PHD', 'BRU', 'LBRU', 'Z', 'SP', 'A', 'X', 'Y', 'finish', 'S', 'z', 'a', 'sp', 'bra', 'phx', 'taz', 'rtn', 'map', 'eom']:
            for kind in ['plain', 'absolute', 'cheap']:
                target = ('::' if kind == 'absolute' else '@' if kind == 'cheap' else '') + name
                before = 'entry:\n' if kind == 'cheap' else ''
                reference = target if kind != 'plain' else '::' + name
                out[f'{cpu}_{kind}_{name}'] = prefix + before + target + ': .byte 7\n.word ' + reference + '\n'
    out.update({
        'inactive_mnemonic': '.if 0\nlda: .byte 7\n.endif\n.byte 8\n',
        'unused_macro': '.macro unused\nlda: .byte 7\n.endmacro\n.byte 8\n',
        'zero_repeat': '.repeat 0\nlda: .byte 7\n.endrepeat\n.byte 8\n',
        'invoked_macro': '.macro broken\nlda: .byte 7\n.endmacro\nbroken\n',
        'generated': '.ident("lda"): .byte 7\n',
        'defined': '.define NAME lda\nNAME: .byte 7\n',
        'cpu_stack': '.p4510\n.pushcpu\n.p02\nINA: .byte 7\n.popcpu\n.byte 8\n',
        'ubiquitous': '.feature ubiquitous_idents\nlda: .byte 7\n.word lda\n',
    })
    return out


if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('capture', Path(__file__).parents[1] / '2026-10-19-ca65-scope-binding/probe.py')
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
