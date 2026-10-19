"""Measure directive tokens, end labels and capture/character input boundaries."""
import importlib.util
from pathlib import Path

FILES = {'ending.inc': '.word finish\nfinish:.end,$100000000\n.byte 8\n'}


def cases() -> dict[str, str]:
    forms = ['.end $100000000', '.end,$100000000', '.end($100000000)', '.end"unread"', '.end:$100000000', '.end+$100000000', '.end/$100000000', '.END[$100000000]', '.end$100000000', '.end%100', '.end.unknown', '.end_thing', '.endmacro', '.end2']
    out = {f'token_{i}': '.byte 7\n' + value + '\n.byte 8\n' for i, value in enumerate(forms)}
    out.update({
        'named': '.word finish\nfinish:.end\n.byte 8\n',
        'named_spaced': '.word finish\n  finish : .end\n.byte 8\n',
        'named_tail': '.word finish\nfinish:.end,$100000000\n.byte 8\n',
        'ident_label': '.word finish\n.ident("finish"):.end\n.byte 8\n',
        'ident_tail': '.word finish\n.ident("finish"):.end .sprintf("%d",4294967300)\n.byte 8\n',
        'ident_spaced': '.word finish\n.ident( "finish" ) : .end\n.byte 8\n',
        'define_label': '.define NAME finish\n.word finish\nNAME:.end\n.byte 8\n',
        'define_tail': '.define NAME finish\n.word finish\nNAME:.end .sprintf("%d",4294967300)\n.byte 8\n',
        'cheap_label': 'entry:\n.word @finish\n@finish:.end\n.byte 8\n',
        'anonymous_label': '.word :+\n:.end\n.byte 8\n',
        'absolute_label': '.org $3000\n.word finish\nfinish:.end\n.byte 8\n',
        'qualified_label': '.scope outer\n.endscope\n.word outer::finish\nouter::finish:.end\n.byte 8\n',
        'include_label': '.include "ending.inc"\n.byte 9\n',
        'macro_label': '.macro stop\n.word finish\nfinish:.end\n.endmacro\nstop\n.byte 8\n',
        'macro_local_label': '.macro stop\n.local finish\n.word finish\nfinish:.end\n.endmacro\nstop\n.byte 8\n',
        'repeat_label': '.repeat 2\n.word finish\nfinish:.end\n.endrepeat\n.byte 8\n',
        'zero_label': '.repeat 0\nfinish:.end\n.endrepeat\n.byte .defined(finish)\n',
        'inactive_label': '.if 0\nfinish:.end\n.endif\n.byte .defined(finish)\n',
        'inactive_bad_tail': '.if 0\nfinish:.end,$100000000\n.endif\n.byte 7\n',
        'macro_bad_tail': '.macro stop\nfinish:.end,$100000000\n.endmacro\nstop\n',
        'macro_unused_bad_tail': '.macro stop\nfinish:.end,$100000000\n.endmacro\n.byte 7\n',
        'repeat_zero_bad_tail': '.repeat 0\nfinish:.end,$100000000\n.endrepeat\n.byte 7\n',
        'alias_end_bad_format': '.define STOP .end\nSTOP .sprintf("%bad",0)\n.byte 8\n',
        'alias_end_large_token': '.define STOP .end\nSTOP .sprintf("%d",4294967300)\n.byte 8\n',
        'named_alias_end': '.define STOP .end\n.word finish\nfinish: STOP .sprintf("%bad",0)\n.byte 8\n',
        'spaced_plain_label': 'finish : .byte 7\n.word finish\n',
        'spaced_mnemonic_label': 'lda : .byte 7\n.word lda\n',
        'operand_colon': 'lda value:\n.byte 7\n',
        'root_unclosed_tail': '.byte 7\n.end\n.if 0\n',
        'same_line_conditional_tail': '.byte 7\n.end:.if 0\n',
        'macro_unclosed_tail': '.macro stop\n.byte 7\n.end\n.if 0\n.endmacro\nstop\n',
    })
    return out


if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('capture', Path(__file__).parents[1] / '2026-10-19-ca65-scope-binding/probe.py')
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.FILES = FILES
    runner.main()
