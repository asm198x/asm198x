"""Retain delimiter, quote, lazy meaning and flat-CPU numeric controls."""
import importlib.util
from pathlib import Path

FILES = {'numeric.inc': '.if 0\n.byte <$100000000\n.endif\n.byte 7\n'}


def cases() -> dict[str, str]:
    out = {
        'else_tail_true': '.if 1\n.byte 7\n.else $100000000\n.byte 8\n.endif\n',
        'else_tail_false': '.if 0\n.byte 7\n.else $100000000\n.byte 8\n.endif\n',
        'elseif_tail_skipped': '.if 1\n.byte 7\n.elseif $100000000\n.byte 8\n.endif\n',
        'elseif_nested_closer': '.if 0\n.byte 7\n.elseif 0\n.byte 8\n.else\n.byte 9\n.endif $100000000\n',
        'nested_else_tail': '.if 1\n.if 0\n.byte 1\n.endif\n.else $100000000\n.byte 8\n.endif\n',
        'empty_else_tail': '.if 0\n.else $100000000\n.endif\n.byte 7\n',
        'comments_before_closer': '.if 0\n; comment\n\n.endif $100000000\n.byte 7\n',
        'lazy_include_argument': '.if 0\n.include 3\n.endif\n.byte 7\n',
        'lazy_computed_include': '.if 0\n.include .sprintf("absent-%u.inc",4294967299)\n.endif\n.byte 7\n',
        'lazy_cpu': '.if 0\n.setcpu "not-a-cpu"\n.endif\n.byte 7\n',
        'lazy_undefined': '.if 0\n.byte <MISSING\n.endif\n.byte 7\n',
        'lazy_syntax': '.if 0\n.byte 1,,\n.endif\n.byte 7\n',
        'lazy_definition': '.if 0\n.define VALUE 1\n.endif\n.byte .defined(VALUE)\n',
        'quoted_apostrophe': ".if 0\n.byte ''' , $FFFFFFFF\n.endif\n.byte 7\n",
        'apostrophe_before_overflow': ".if 0\n.byte ''' , $100000000\n.endif\n.byte 7\n",
        'double_quote_character': ".if 0\n.byte '\"' , $100000000\n.endif\n.byte 7\n",
        'dollar_character': ".if 0\n.byte '$' , $100000000\n.endif\n.byte 7\n",
        'token_count_overflow': '.byte .tcount({$100000000})\n',
        'token_match_overflow': '.byte .match({4294967300},{0})\n',
        'unused_macro_end_format': '.macro finish\n.end .sprintf("%bad",0)\n.endmacro\nfinish\n.byte 8\n',
        'included_overflow': '.include "numeric.inc"\n',
    }
    numbers = ['4294967299', '4294967300', '$FFFFFFFF', '$100000000', '0FFFFFFFFh', '100000000h', '%' + '1' * 32, '%1' + '0' * 32]
    for cpu in ['65816', 'huc6280']:
        for index, number in enumerate(numbers):
            contexts = {
                'unused_macro': f'.macro unused\n.byte <{number}\n.endmacro\n.byte 7\n',
                'called_macro': f'.macro emit\n.byte <{number}\n.endmacro\nemit\n',
                'unused_argument': f'.macro emit operand\n.byte 7\n.endmacro\nemit {number}\n',
                'quoted': f'.byte "{number}"\n',
                'comment': f'; {number}\n.byte 7\n',
                'token_count': f'.byte .tcount({{{number}}})\n',
            }
            for context, source in contexts.items():
                out[f'{cpu}_{context}_{index}'] = f'.setcpu "{cpu}"\n' + source
    return out


if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('capture', Path(__file__).parents[1] / '2026-10-19-ca65-scope-binding/probe.py')
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.FILES = FILES
    runner.main()
