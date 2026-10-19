"""Compare an explicit executable with every selected numeric native control."""
import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('binary', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    facts = json.loads(Path(__file__).with_name('selected-conversion-native.json').read_text())
    assert len(facts['cases']) == 180
    rows = []
    with tempfile.TemporaryDirectory(prefix='asm198x-numeric-conversion-') as td:
        root = Path(td)
        for fact in facts['cases']:
            root.joinpath('input.s').write_text(fact['source'])
            root.joinpath('input.cfg').write_text(fact['config'])
            root.joinpath('out.bin').write_bytes(b'previous image')
            argv = [str(binary), '--dialect', fact['dialect']]
            if fact['dialect'] == 'ca65':
                argv += ['-C', 'input.cfg']
            argv += ['input.s', '-o', 'out.bin', '--debug=out.debug198x', '--message-format=json']
            p = subprocess.run(argv, cwd=root, capture_output=True, text=True, timeout=10, check=False)
            accepted = fact['native'][-1]['exit'] == 0
            data = root.joinpath('out.bin').read_bytes()
            matches = p.returncode == (0 if accepted else 1) and (data.hex() == fact['native_hex'] if accepted else data == b'previous image')
            rows.append({'name': fact['name'], 'dialect': fact['dialect'], 'argv': argv, 'exit': p.returncode, 'stdout': p.stdout, 'stderr': p.stderr, 'hex': data.hex(), 'matches_native': matches})
    args.output.write_text(json.dumps({'binary': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'results': rows}, indent=2) + '\n')
    print('cases', len(rows), 'differences', sum(not r['matches_native'] for r in rows))


if __name__ == '__main__':
    main()
