"""Measure native brace capture, without assuming balanced nested lists."""
import importlib.util
import json
import sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
def load(name,path):
 spec=importlib.util.spec_from_file_location(name,path);assert spec and spec.loader
 module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
empty=load('empty',ROOT/'docs/plans/2026-10-21-ca65-empty-calls/probe.py')
EXPRESSIONS={
 'one_open':'.tcount({{1})', 'two_open':'.tcount({{{1})',
 'balanced_nested':'.tcount({{1}})', 'embedded_open':'.tcount({1{2})',
 'embedded_balanced':'.tcount({1{2}})', 'empty_nested':'.tcount({{}})',
 'max_control':'.tcount({.max(1,2)})',
 'quoted_open':'.tcount({"{"})', 'quoted_close':'.tcount({"}"})',
 'concat_open':'.tcount({.concat("{")})', 'concat_close':'.tcount({.concat("}")})',
 'opening_match':'.match({{VALUE},{{OTHER})',
 'opening_xmatch':'.xmatch({{VALUE},{{OTHER})',
 'opening_same_xmatch':'.xmatch({{VALUE},{{VALUE})',
 'nested_match':'.tcount({.match({1},{2})})',
}
def cases():
 out={}
 for dialect in ('ca65','65816','huc6280'):
  for name,expr in EXPRESSIONS.items():empty.add_contexts(out,dialect,name,'VALUE=7\nOTHER=8\n.byte '+expr+'\n')
 assert len(out)==225;return out
if __name__=='__main__':
 capture=load('capture',ROOT/'docs/plans/2026-10-19-ca65-scope-binding/probe.py')
 prepared=cases();capture.cases=lambda:prepared;capture.FILES=empty.FILES;capture.main()
 path=Path(sys.argv[1]);facts=json.loads(path.read_text());assert len(facts['cases'])==225
 for case in facts['cases']:case['dialect']=case['name'].split('_',1)[0]
 path.write_text(json.dumps(facts,indent=2)+'\n')
