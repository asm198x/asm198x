"""Compare an explicit executable with native bytes and logical debug values."""
import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

from build_fixture import native_facts


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('binary', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    facts = native_facts()
    rows = []
    with tempfile.TemporaryDirectory(prefix='asm198x-logical-debug-compare-') as td:
        root = Path(td)
        known = ['input.s', *facts['files']]

        def normalize(name: str) -> str:
            name = name.replace('\\', '/')
            return next((p for p in known if name == p or name.endswith('/' + p)), name)

        for name, text in facts['files'].items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        for fact in facts['cases']:
            root.joinpath('input.s').write_text(fact['source'])
            root.joinpath('input.cfg').write_text(fact['config'])
            root.joinpath('out.bin').write_bytes(b'previous image')
            argv = [str(binary), '--dialect', 'ca65', '-C', 'input.cfg', 'input.s', '-o', 'out.bin', '--debug=out.debug198x', '--sym=out.sym', '--listing=out.lst', '--listing-json=out.lst.json', '--message-format=json']
            p = subprocess.run(argv, cwd=root, capture_output=True, text=True, timeout=10, check=False)
            accepted = fact['native'][-1]['exit'] == 0
            data = root.joinpath('out.bin').read_bytes()
            matches = p.returncode == (0 if accepted else 1) and (data.hex() == fact['native_hex'] if accepted else data == b'previous image')
            assert matches, (fact['name'], p.stderr, p.stdout)
            records = [json.loads(line) for line in root.joinpath('out.debug198x').read_text().splitlines()] if accepted else []
            for record in records:
                if record['t'] == 'header':
                    record['sources'] = [normalize(name) for name in record['sources']]
                if record['t'] == 'line':
                    record['file'] = normalize(record['file'])
            sections = {r['id']: r for r in records if r['t'] == 'section'}
            labels = {r['name']: r for r in records if r['t'] == 'symbol' and r['kind'] == 'label'}
            assert set(labels) == set(fact['symbol_values']), (fact['name'], labels, fact['symbol_values'])
            mismatches = []
            for name, value in fact['symbol_values'].items():
                symbol = labels[name]
                section = sections[symbol['section']]
                actual = section['base'] + symbol['offset'] if section.get('base') is not None else None
                expected = None if name in fact['non_cpu_symbols'] else value
                if actual != expected:
                    mismatches.append({'name': name, 'native': expected, 'actual': actual})
            if accepted and fact['name'] == 'pc_data_lines':
                payload = json.loads(p.stdout)
                for section in payload['section_debug']:
                    for line in section['debug']['lines']:
                        file_at = section['file_offset'] + line['offset']
                        expected = int.from_bytes(data[file_at:file_at + 2], 'little')
                        actual = section['section']['base'] + line['offset']
                        if actual != expected:
                            mismatches.append({'line': line['line'], 'native_pc_word': expected, 'actual': actual})
            listing = json.loads(root.joinpath('out.lst.json').read_text()) if accepted else None
            rows.append({'name': fact['name'], 'argv': argv, 'exit': p.returncode, 'stdout': p.stdout, 'stderr': p.stderr, 'hex': data.hex(), 'bytes_match': matches, 'normalized_records': records, 'listing': listing, 'metadata_mismatches': mismatches})
    args.output.write_text(json.dumps({'binary': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'results': rows}, indent=2) + '\n')
    print('cases', len(rows), 'byte mismatches', sum(not r['bytes_match'] for r in rows), 'metadata mismatches', sum(bool(r['metadata_mismatches']) for r in rows))


if __name__ == '__main__':
    main()
