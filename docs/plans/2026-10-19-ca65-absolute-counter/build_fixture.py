"""Retain the native counter and address controls without selection by outcome."""
import json
from pathlib import Path

root = Path(__file__).parent
facts = json.loads((root / 'native-base.json').read_text())
more = json.loads((root / 'native.json').read_text())
assert {k: v['sha256'] for k, v in facts['tools'].items()} == {k: v['sha256'] for k, v in more['tools'].items()}
facts['files'] = more['files']
facts['cases'] += more['cases']
assert len(facts['cases']) == len({c['name'] for c in facts['cases']}) == 63
assert sum(c['native'][-1]['exit'] == 0 for c in facts['cases']) == 62
assert all(all(s['exit'] in (0, 1) for s in c['native']) for c in facts['cases'])
text = json.dumps(facts, indent=2) + '\n'
(root / 'selected-native.json').write_text(text)
(root.parents[2] / 'crates/asm198x/tests/fixtures/ca65-absolute-counter.json').write_text(text)
print('63 completed native cases: 62 accepted, 1 refused')
