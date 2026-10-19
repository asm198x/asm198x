"""Bound actual predecessor/current commands and retain formatter source loss."""
import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).parent


def run(argv: list[str], root: Path) -> dict:
    try:
        p = subprocess.run(argv, cwd=root, capture_output=True, text=True, timeout=5, check=False)
        return {'argv': argv, 'exit': p.returncode, 'stdout': p.stdout, 'stderr': p.stderr, 'timed_out': False}
    except subprocess.TimeoutExpired as e:
        def text(value: str | bytes | None) -> str:
            return value.decode(errors='replace') if isinstance(value, bytes) else value or ''
        return {'argv': argv, 'exit': None, 'stdout': text(e.stdout), 'stderr': text(e.stderr), 'timed_out': True}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('binary', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    facts = json.loads((ROOT / 'native.json').read_text())
    rows = []
    with tempfile.TemporaryDirectory(prefix='asm198x-repeat-end-') as td:
        root = Path(td)
        for name, source in facts['files'].items():
            root.joinpath(name).write_text(source)
        for case in facts['cases']:
            root.joinpath('input.s').write_text(case['source'])
            root.joinpath('input.cfg').write_text(case['config'])
            root.joinpath('out.bin').write_bytes(b'previous image')
            argv = [str(binary), '--dialect', 'ca65', '-C', 'input.cfg', 'input.s', '-o', 'out.bin', '--debug=out.debug198x']
            result = run(argv, root)
            accepted = case['native'][-1]['exit'] == 0
            image = root.joinpath('out.bin').read_bytes()
            matches = result['exit'] == (0 if accepted else 1) and (image.hex() == case['native_hex'] if accepted else image == b'previous image')
            formatted = None
            if accepted:
                formatted = run([str(binary), 'fmt', '--dialect', 'ca65', 'input.s'], root)
                root.joinpath('formatted.s').write_text(formatted['stdout'])
                has_end = lambda s: any(line.strip().split()[:1] == ['.end'] for line in s.splitlines())
                formatted['lost_end'] = has_end(case['source']) and not has_end(formatted['stdout'])
                if not formatted['lost_end']:
                    stages = [run(['/opt/homebrew/bin/ca65', 'formatted.s', '-o', 'formatted.o'], root)]
                    if stages[0]['exit'] == 0:
                        stages.append(run(['/opt/homebrew/bin/ld65', '-C', 'input.cfg', 'formatted.o', '-o', 'formatted.bin'], root))
                    formatted['native'] = stages
                    formatted['native_hex'] = root.joinpath('formatted.bin').read_bytes().hex() if stages[-1]['exit'] == 0 else None
                    assert formatted['native_hex'] == case['native_hex'], case['name']
            rows.append({'name': case['name'], 'assembly': result, 'hex': image.hex(), 'matches_native': matches, 'formatter': formatted})
    args.output.write_text(json.dumps({'binary': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'results': rows}, indent=2) + '\n')
    print('cases', len(rows), 'differences', sum(not r['matches_native'] for r in rows), 'timeouts', sum(r['assembly']['timed_out'] for r in rows), 'formatter source loss', sum(bool(r['formatter'] and r['formatter']['lost_end']) for r in rows))


if __name__ == '__main__':
    main()
