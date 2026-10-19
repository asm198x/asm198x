"""Measure native delimiter-adjacent whitespace without guessing its alphabet."""
import importlib.util
from pathlib import Path


def cases() -> dict[str, str]:
    out = {}
    for tag, space in [('space', ' '), ('tab', '\t'), ('mixed', ' \t '), ('vertical', '\v'), ('formfeed', '\f'), ('carriage', '\r'), ('nbsp', '\u00a0'), ('emspace', '\u2003')]:
        for name in ['finish', '@finish', 'lda', 'a', 'Z']:
            before = 'entry:\n' if name.startswith('@') else ''
            out[f'{tag}_{name}'] = before + name + space + ': .byte 7\n'
    out.update({
        'anonymous_space': ' : .byte 7\n.word :-\n',
        'plain_reference': '.word finish\nfinish \t :nop\n',
        'cheap_reference': 'entry:\n.word @finish\n@finish \t :nop\n',
        'macro_head': 'entry : .macro run\n.byte 7\n.endmacro\nrun\n.word entry\n',
        'macro_call': '.macro emit\n.byte 7\n.endmacro\nentry :emit\n.word entry\n',
        'conditional_head': 'entry : .if 1\n.byte 7\n.endif\n.word entry\n',
        'repeat_head': 'entry : .repeat 2\n.byte 7\n.endrepeat\n.word entry\n',
        'scope_head': 'entry : .scope outer\n.byte 7\n.endscope\n.word entry\n',
        'quoted_colon': '.byte "x : y"\n',
        'operand_colon': 'lda value :\n.byte 7\n',
        'scope_separator': '.scope outer\nvalue=7\n.endscope\n.byte outer :: value\n',
        'split_identifier': 'fin ish : .byte 7\n',
        'comment_boundary': 'finish ;comment\n: .byte 7\n',
        'inactive_mnemonic': '.if 0\nlda : .byte 7\n.endif\n.byte 8\n',
        'literal_end': '.byte 7\n.end : .if 0\n',
    })
    return out


if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('capture', Path(__file__).parents[1] / '2026-10-19-ca65-scope-binding/probe.py')
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
