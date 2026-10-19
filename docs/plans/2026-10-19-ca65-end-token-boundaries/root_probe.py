"""Keep #842 token recognition independent of the #830 label grammar."""
import importlib.util
from pathlib import Path

FILES = {'ending.inc': '.byte 7\n.end,$100000000\n.byte 8\n'}


def cases() -> dict[str, str]:
    forms = ['.end $100000000', '.end,$100000000', '.end($100000000)', '.end"unread"', '.end:$100000000', '.end+$100000000', '.end/$100000000', '.END[$100000000]', '.end$100000000', '.end%100', '.end.unknown', '.end_thing', '.endmacro', '.end2']
    out = {f'token_{i}': '.byte 7\n' + value + '\n.byte 8\n' for i, value in enumerate(forms)}
    out.update({
        'same_line_conditional_tail': '.byte 7\n.end:.if 0\n',
        'include_end': '.include "ending.inc"\n.byte 9\n',
        'macro_legal_tail': '.macro stop\n.byte 7\n.end,1\n.endmacro\nstop\n.byte 8\n',
        'repeat_legal_tail': '.repeat 2\n.byte 7\n.end(1)\n.endrepeat\n.byte 8\n',
        'inactive_bad_tail': '.if 0\n.end,$100000000\n.endif\n.byte 7\n',
        'repeat_zero_bad_tail': '.repeat 0\n.end,$100000000\n.endrepeat\n.byte 7\n',
        'macro_unused_bad_tail': '.macro stop\n.end,$100000000\n.endmacro\n.byte 7\n',
        'comment_end': '.byte 7 ; .end,$100000000\n.byte 8\n',
        'quoted_end': '.byte ".end,$100000000"\n.byte 7\n',
        'trailing_comment': '.byte 7\n.end; $100000000\n.byte 8\n',
        'ordinary_bytes': '.byte 7\n.byte 8\n',
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
