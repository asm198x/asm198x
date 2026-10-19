"""Retain native symbol values and physical debug spans for logical addresses."""
import argparse
import importlib.util
import json
import re
import subprocess
import tempfile
from pathlib import Path

CONFIG = 'MEMORY { HEADER:start=0,size=$10,file=%O; MAIN:start=$1000,size=$7000,file=%O; RAM:start=$20,size=$100,file=""; PPU:start=0,size=$2000,file=%O; }\nSEGMENTS { HEADER:load=HEADER,type=ro; CODE:load=MAIN,type=ro; UNUSED:load=MAIN,type=ro; RODATA:load=MAIN,type=ro; ZEROPAGE:load=RAM,type=zp; BSS:load=RAM,type=bss; CHARS:load=PPU,type=ro; }\n'
FILES = {'sub/data.inc': 'included:.byte 7\n', 'sub/cycles.inc': 'routine:nop\n.org $3000\nnop\n.reloc\nnop\n'}
CONFIG_IDS = {name: index for index, name in enumerate(['HEADER', 'CODE', 'UNUSED', 'RODATA', 'ZEROPAGE', 'BSS', 'CHARS'])}


def cases() -> dict[str, str]:
    spec = importlib.util.spec_from_file_location('base_cases', Path(__file__).with_name('probe.py'))
    assert spec is not None and spec.loader is not None
    base = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(base)
    out = base.cases()
    out.update({
        'zero_origin': '.org 0\nitem:.byte 7\n.word item\n',
        'no_origin': 'item:.byte 7\n.word item\n',
        'alignment': '.org $2000\nfirst:.byte 7\n.align 4\nsecond:.word first,second\n',
        'alias': '.org $2000\nitem:.byte 7\nALIAS=item\n.word item,ALIAS\n',
        'include': '.org $2000\n.include "sub/data.inc"\n.word included\n',
        'macro': '.macro emit\nitem:.byte 7\n.endmacro\n.org $2000\nemit\n.word item\n',
        'repeat': '.org $2000\n.repeat 3,I\n.ident(.sprintf("item%d",I)):.byte I\n.endrepeat\n.word item0,item1,item2\n',
        'multiple_cheap': '.org $2000\nfirst:\n@item:.byte 7\nsecond:\n@item:.byte 8\n.word @item\n',
        'cycle_aliases': '.org $2000\nroutine:\nalias:nop\n.org $3000\nnop\n.reloc\nnop\n',
        'cycle_multi_segment': '.org $2000\nroutine:nop\n.segment "RODATA"\nother:nop\n.segment "CODE"\nnop\n',
        'cycle_before_org': 'routine:nop\n.org $2000\nnop\n.reloc\nnop\n',
        'cycle_scope': '.org $2000\n.proc outer\nroutine:nop\n.org $3000\nnop\n.reloc\nnop\n.endproc\n',
        'pc_data_lines': '.org $2000\n.word *\n.segment "RODATA"\n.word *\n.segment "CODE"\n.word *\n.reloc\n.word *\n',
        'noncpu_header': '.segment "HEADER"\n.org $2000\nheader:.byte 7\n.segment "CODE"\ncode:.word header\n',
        'noncpu_chars': '.segment "CHARS"\n.org $20\ntile:.byte 7\n.segment "CODE"\ncode:.word tile\n',
        'no_file': '.segment "BSS"\n.org $2000\nbuffer:.res 3\n.segment "CODE"\ncode:.word buffer\n',
        'label_only_span': '.org $2000\nitem:\n.segment "RODATA"\n.word item\n',
        'overlap': '.org $2000\nfirst:nop\n.org $2000\nsecond:nop\n.word first,second\n',
        'included_cycle': '.org $2000\n.include "sub/cycles.inc"\n',
        'many_spans': '.repeat 64,I\n.org $2000+I*4\n.ident(.sprintf("item%d",I)):.byte I\n.word .ident(.sprintf("item%d",I))\n.endrepeat\n',
        'reserved_names': '.org $2000\nconstructor:.byte 7\n__proto__:.byte 8\n.word constructor,__proto__\n',
        'empty_absolute': '.org $2000\n',
        'duplicate': '.org $2000\nitem:.byte 7\nitem:.byte 8\n',
    })
    return out


def records(text: str) -> dict[str, list[dict[str, str]]]:
    out = {}
    for line in text.splitlines():
        tag, sep, fields = line.partition('\t')
        if not sep:
            continue
        row = {}
        for field in re.findall(r'(?:[^,"]|"(?:[^"\\]|\\.)*")+', fields):
            key, value = field.split('=', 1)
            row[key] = json.loads(value) if value.startswith('"') else value
        out.setdefault(tag, []).append(row)
    return out


def metadata(raw: str) -> tuple[dict[str, int], dict[str, int], list[dict], dict[str, list[dict]]]:
    data = records(raw)
    scopes = {int(r['id']): r for r in data.get('scope', [])}
    symbols = {int(r['id']): r for r in data.get('sym', [])}

    def scope_name(index: int) -> str:
        names = []
        seen = set()
        while index in scopes:
            assert index not in seen
            seen.add(index)
            row = scopes[index]
            if row['name']:
                names.append(row['name'])
            if 'parent' not in row:
                break
            index = int(row['parent'])
        return '::'.join(reversed(names))

    def symbol_name(index: int, seen: frozenset[int] = frozenset()) -> str:
        assert index not in seen
        row = symbols[index]
        if 'parent' in row:
            return symbol_name(int(row['parent']), seen | {index}) + row['name']
        prefix = scope_name(int(row.get('scope', '0')))
        return prefix + '::' + row['name'] if prefix else row['name']

    values = {symbol_name(i): int(row['val'], 0) for i, row in symbols.items() if row.get('type') == 'lab' and 'val' in row}
    assert len(values) == sum(row.get('type') == 'lab' and 'val' in row for row in symbols.values())
    constants = {symbol_name(i): int(row['val'], 0) for i, row in symbols.items() if row.get('type') == 'equ' and 'val' in row}
    files = {int(r['id']): r['name'] for r in data.get('file', [])}
    segments = {int(r['id']): r for r in data.get('seg', [])}
    spans = {int(r['id']): r for r in data.get('span', [])}
    physical = []
    for line in data.get('line', []):
        for index in line.get('span', '').split('+'):
            if not index:
                continue
            span = spans[int(index)]
            segment = segments[int(span['seg'])]
            if 'ooffs' in segment:
                physical.append({'file': files[int(line['file'])], 'line': int(line['line']), 'file_offset': int(segment['ooffs'], 0) + int(span['start'], 0), 'length': int(span['size'], 0)})
    lines = {int(row['id']): row for row in data.get('line', [])}
    definitions = {}
    for index, row in symbols.items():
        if row.get('type') == 'lab' and 'val' in row:
            definitions[symbol_name(index)] = [
                {'file': files[int(lines[int(i)]['file'])], 'line': int(lines[int(i)]['line'])}
                for i in row.get('def', '').split('+') if i
            ]
    return values, constants, physical, definitions


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    rows = []
    with tempfile.TemporaryDirectory(prefix='asm198x-native-logical-debug-') as td:
        root = Path(td)
        for name, text in FILES.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        root.joinpath('input.cfg').write_text(CONFIG)

        def run(argv: list[str]) -> dict:
            p = subprocess.run(argv, cwd=root, capture_output=True, text=True, timeout=10, check=False)
            return {'argv': argv, 'cwd': str(root), 'exit': p.returncode, 'stdout': p.stdout, 'stderr': p.stderr}

        import hashlib
        tools = {name: {'path': '/opt/homebrew/bin/' + name, 'sha256': hashlib.sha256(Path('/opt/homebrew/bin', name).read_bytes()).hexdigest(), 'version': run(['/opt/homebrew/bin/' + name, '--version'])} for name in ['ca65', 'ld65']}
        for name, source in cases().items():
            config = CONFIG.replace('RAM:start=$20', 'RAM:start=0') if name == 'noncpu_header' else CONFIG
            root.joinpath('input.cfg').write_text(config)
            root.joinpath('input.s').write_text(source)
            for output in ['input.o', 'native.bin', 'native.dbg', 'native.lst']:
                root.joinpath(output).unlink(missing_ok=True)
            stages = [run(['/opt/homebrew/bin/ca65', '-g', 'input.s', '-o', 'input.o', '-l', 'native.lst'])]
            if stages[-1]['exit'] == 0:
                stages.append(run(['/opt/homebrew/bin/ld65', '-C', 'input.cfg', 'input.o', '-o', 'native.bin', '--dbgfile', 'native.dbg']))
            debug = root.joinpath('native.dbg').read_text() if root.joinpath('native.dbg').exists() else ''
            values, constants, physical, definitions = metadata(debug)
            rows.append({'name': name, 'source': source, 'config': config, 'native': stages, 'native_hex': root.joinpath('native.bin').read_bytes().hex() if root.joinpath('native.bin').exists() else None, 'native_debug': debug, 'native_listing': root.joinpath('native.lst').read_text() if root.joinpath('native.lst').exists() else '', 'symbol_values': values, 'constant_values': constants, 'physical_lines': physical, 'symbol_definitions': definitions})
    assert all(all(s['exit'] in (0, 1) for s in row['native']) for row in rows)
    args.output.write_text(json.dumps({'tools': tools, 'files': FILES, 'config_ids': CONFIG_IDS, 'cases': rows}, indent=2) + '\n')
    print('cases', len(rows), 'accepted', sum(row['native'][-1]['exit'] == 0 for row in rows), 'native labels', sum(len(row['symbol_values']) for row in rows))


if __name__ == '__main__':
    main()
