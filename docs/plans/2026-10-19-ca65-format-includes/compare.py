"""Compare native assembly and read-only formatter requests independently."""
import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).parent


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('binary', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    facts = json.loads((ROOT / 'native.json').read_text())
    rows = []
    with tempfile.TemporaryDirectory(prefix='asm198x-format-include-') as td:
        root = Path(td)
        for name, source in facts['files'].items():
            root.joinpath(name).write_text(source)
        for case in facts['cases']:
            dialect = '65816' if case['name'].startswith('65816_') else 'huc6280' if case['name'].startswith('huc6280_') else 'ca65'
            root.joinpath('input.s').write_text(case['source'])
            root.joinpath('input.cfg').write_text(case['config'])
            root.joinpath('out.bin').write_bytes(b'previous image')
            argv = [str(binary), '--dialect', dialect] + (['-C', 'input.cfg'] if dialect == 'ca65' else []) + ['input.s', '-o', 'out.bin', '--debug=out.debug198x']
            p = subprocess.run(argv, cwd=root, capture_output=True, text=True, timeout=5, check=False)
            accepted = case['native'][-1]['exit'] == 0
            image = root.joinpath('out.bin').read_bytes()
            matches = p.returncode == (0 if accepted else 1) and (image.hex() == case['native_hex'] if accepted else image == b'previous image')
            formatted = subprocess.run([str(binary), 'fmt', '--dialect', dialect, 'input.s'], cwd=root, capture_output=True, text=True, timeout=5, check=False)
            rows.append({'name': case['name'], 'dialect': dialect, 'assembly': {'argv': argv, 'exit': p.returncode, 'stdout': p.stdout, 'stderr': p.stderr}, 'hex': image.hex(), 'matches_native': matches, 'formatter': {'exit': formatted.returncode, 'stdout': formatted.stdout, 'stderr': formatted.stderr}})
    args.output.write_text(json.dumps({'binary': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'results': rows}, indent=2) + '\n')
    print('cases', len(rows), 'assembly differences', sum(not r['matches_native'] for r in rows), 'formatter refusals', sum(r['formatter']['exit'] != 0 for r in rows))


if __name__ == '__main__':
    main()
