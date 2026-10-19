"""Keep every native metadata control and the actual no-origin baseline."""
import json
from pathlib import Path

ROOT = Path(__file__).parent


def native_facts() -> dict:
    facts = json.loads((ROOT / 'native-symbols.json').read_text())
    assert len(facts['cases']) == 39
    assert sum(c['native'][-1]['exit'] == 0 for c in facts['cases']) == 38
    assert sum(len(c['symbol_values']) for c in facts['cases']) == 121
    assert all(all(s['exit'] in (0, 1) for s in c['native']) for c in facts['cases'])
    costs = {
        'cycle_aliases': {'routine': (6, 3), 'alias': (6, 3)},
        'cycle_multi_segment': {'routine': (4, 2), 'other': (2, 1)},
        'cycle_before_org': {'routine': (6, 3)},
        'cycle_scope': {'outer': (6, 3), 'outer::routine': (6, 3)},
        'included_cycle': {'routine': (6, 3)},
        'overlap': {'first': (2, 1), 'second': (2, 5)},
    }
    for case in facts['cases']:
        case['non_cpu_symbols'] = {'header': 'HEADER'} if case['name'] == 'noncpu_header' else {'tile': 'CHARS'} if case['name'] == 'noncpu_chars' else {}
        case['no_file_symbols'] = ['buffer'] if case['name'] == 'no_file' else []
        case['metadata_only_symbols'] = ['item'] if case['name'] == 'label_only_span' else []
        case['cycle_costs'] = {name: {'min': cycles, 'max': cycles, 'bytes': size} for name, (cycles, size) in costs.get(case['name'], {}).items()}
    return facts


def main() -> None:
    facts = native_facts()
    preceding = json.loads((ROOT / 'predecessor-debug.json').read_text())
    assert len(preceding['results']) == 39
    assert all(c['bytes_match'] for c in preceding['results'])
    rows = {c['name']: c for c in preceding['results']}
    known = ['input.s', *facts['files']]
    def relative(name: str) -> str:
        name = name.replace('\\', '/')
        return next((p for p in known if name == p or name.endswith('/' + p)), name)
    total_lines = 0
    for case in facts['cases']:
        previous = rows[case['name']]
        case['expected_lines'] = []
        if previous['exit'] == 0:
            for line in previous['listing']['lines']:
                assert line['file_offset'] is not None
                case['expected_lines'].append({'file': relative(line['file']), 'line': line['line'], 'file_offset': line['file_offset'], 'length': line['bytes'], 'cycles': line.get('cycles')})
            total_lines += len(case['expected_lines'])
            observed = {c['name']: c for c in previous['listing']['labels']}
            for name, expected in case['cycle_costs'].items():
                assert observed[name]['cycles'] == {'min': expected['min'], 'max': expected['max']}
                assert observed[name]['bytes'] == expected['bytes']
            constants = {s['name']: s['value'] for s in previous['normalized_records'] if s['t'] == 'symbol' and s['kind'] == 'const'}
            assert constants == case['constant_values'], (case['name'], constants, case['constant_values'])
    assert total_lines == 213
    baseline = next(c for c in preceding['results'] if c['name'] == 'no_origin')
    assert baseline['normalized_records'] and not baseline['metadata_mismatches']
    facts['no_origin_records'] = baseline['normalized_records']
    text = json.dumps(facts, indent=2) + '\n'
    (ROOT / 'selected-native.json').write_text(text)
    (ROOT.parents[2] / 'crates/asm198x/tests/fixtures/ca65-absolute-debug.json').write_text(text)
    print('39 native cases; 121 label values; no-origin baseline retained')


if __name__ == '__main__':
    main()
