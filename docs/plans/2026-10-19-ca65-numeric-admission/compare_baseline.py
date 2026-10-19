"""Retain source admission and formatter outcomes from the clean predecessor."""
import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).parent
PREVIOUS = ROOT.parent / '2026-10-19-ca65-numeric-limits'


def facts() -> dict:
    selected = []
    identities = {}
    for prefix, filename in [('core', 'native.json'), ('admission', 'native-admission.json'), ('boundary', 'native-boundaries.json'), ('order', 'native-order.json')]:
        capture = json.loads((PREVIOUS / filename).read_text())
        identities[prefix] = capture['tools']
        for case in capture['cases']:
            # Character syntax and repeat execution are separate existing gaps.
            if prefix == 'boundary' and case['name'] == 'inactive_26':
                continue
            if prefix == 'order' and case['name'] in ['one_repeat_end_after', 'one_repeat_end_end_after']:
                continue
            selected.append(dict(case, name=prefix + '_' + case['name'], dialect='ca65'))
    assert len(selected) == len({c['name'] for c in selected}) == 206
    assert sum(c['native'][-1]['exit'] == 0 for c in selected) == 102
    extra = json.loads((ROOT / 'native-extra.json').read_text())
    identities['extra'] = extra['tools']
    for case in extra['cases']:
        dialect = '65816' if case['name'].startswith('65816_') else 'huc6280' if case['name'].startswith('huc6280_') else 'ca65'
        selected.append(dict(case, name='extra_' + case['name'], dialect=dialect))
    assert len(selected) == len({c['name'] for c in selected}) == 323
    assert sum(c['native'][-1]['exit'] == 0 for c in selected) == 174
    # Linked .setcpu switching is an existing #302 gap, independent of numeric
    # admission. Flat CPU numeric controls above use actual supported dialects.
    excluded = [c for c in selected if c['name'].startswith('admission_cpu_')]
    selected = [c for c in selected if not c['name'].startswith('admission_cpu_')]
    assert len(excluded) == 6 and len(selected) == 317
    assert sum(c['native'][-1]['exit'] == 0 for c in selected) == 172
    return {'tools': identities, 'files': extra['files'], 'excluded_linked_cpu_switch': excluded, 'cases': selected}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('binary', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    capture = facts()
    rows = []
    with tempfile.TemporaryDirectory(prefix='asm198x-numeric-admission-baseline-') as td:
        root = Path(td)
        for name, text in capture['files'].items():
            root.joinpath(name).write_text(text)
        for case in capture['cases']:
            root.joinpath('input.s').write_text(case['source'])
            root.joinpath('input.cfg').write_text(case['config'])
            root.joinpath('out.bin').write_bytes(b'previous image')
            argv = [str(binary), '--dialect', case['dialect']]
            if case['dialect'] == 'ca65':
                argv += ['-C', 'input.cfg']
            argv += ['input.s', '-o', 'out.bin', '--debug=out.debug198x', '--message-format=json']
            p = subprocess.run(argv, cwd=root, capture_output=True, text=True, timeout=10, check=False)
            accepted = case['native'][-1]['exit'] == 0
            image = root.joinpath('out.bin').read_bytes()
            matches = p.returncode == (0 if accepted else 1) and (image.hex() == case['native_hex'] if accepted else image == b'previous image')
            formatted = None
            if accepted:
                command = [str(binary), 'fmt', '--dialect', case['dialect'], 'input.s']
                result = subprocess.run(command, cwd=root, capture_output=True, text=True, timeout=10, check=False)
                assert result.returncode in (0, 1)
                formatted = {'argv': command, 'exit': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr}
                if result.returncode == 0:
                    root.joinpath('formatted.s').write_text(result.stdout)
                    again = subprocess.run([str(binary), 'fmt', '--dialect', case['dialect'], 'formatted.s'], cwd=root, capture_output=True, text=True, timeout=10, check=False)
                    replay_argv = [a if a != 'input.s' else 'formatted.s' for a in argv]
                    replay = subprocess.run(replay_argv, cwd=root, capture_output=True, text=True, timeout=10, check=False)
                    replay_hex = root.joinpath('out.bin').read_bytes().hex()
                    formatted['idempotent'] = again.returncode == 0 and again.stdout == result.stdout
                    formatted['replay'] = {'argv': replay_argv, 'exit': replay.returncode, 'stdout': replay.stdout, 'stderr': replay.stderr, 'hex': replay_hex}
                    formatted['matches_native'] = formatted['idempotent'] and replay.returncode == 0 and replay_hex == case['native_hex']

            rows.append({'name': case['name'], 'argv': argv, 'exit': p.returncode, 'stdout': p.stdout, 'stderr': p.stderr, 'hex': image.hex(), 'matches_native': matches, 'formatter': formatted})
    args.output.write_text(json.dumps({'binary': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'results': rows}, indent=2) + '\n')
    print('cases', len(rows), 'differences', sum(not r['matches_native'] for r in rows), 'existing formatter refusals', [r['name'] for r in rows if r['formatter'] and r['formatter']['exit'] == 1])


if __name__ == '__main__':
    main()
