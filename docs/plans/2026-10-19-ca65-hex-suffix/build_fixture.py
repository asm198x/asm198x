"""Retain active numeric grammar and native token-value identity together."""
import json
from pathlib import Path

root = Path(__file__).parent
facts = json.loads((root / 'native.json').read_text())
more = json.loads((root / 'native-tokens.json').read_text())
assert {k: v['sha256'] for k, v in facts['tools'].items()} == {k: v['sha256'] for k, v in more['tools'].items()}
facts['cases'] += more['cases']
assert len(facts['cases']) == len({c['name'] for c in facts['cases']}) == 30
assert sum(c['native'][-1]['exit'] == 0 for c in facts['cases']) == 23
assert all(all(s['exit'] in (0, 1) for s in c['native']) for c in facts['cases'])
text = json.dumps(facts, indent=2) + '\n'
(root / 'selected-native.json').write_text(text)
(root.parents[2] / 'crates/asm198x/tests/fixtures/ca65-hex-suffix.json').write_text(text)
print('30 completed native cases: 23 accepted, 7 refused')
