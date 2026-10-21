"""Capture native numeric-call tokens and constructor controls inside lists."""
import importlib.util
import json
import sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
def load(name,path):
 spec=importlib.util.spec_from_file_location(name,path);assert spec and spec.loader
 module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
empty=load('empty',ROOT/'docs/plans/2026-10-21-ca65-empty-calls/probe.py')
CALLS={
 'const':'.const(VALUE)', 'const_missing':'.const(MISSING)',
 'match':'.match({1},{2})', 'xmatch':'.xmatch({1},{1})',
 'ismnem':'.ismnem(lda)', 'ismnemonic':'.ismnemonic(lda)',
 'ismnem_missing':'.ismnem(MISSING)', 'blank':'.blank()',
 'tcount':'.tcount()', 'strlen':'.strlen("A")', 'strat':'.strat("A",0)',
 'sizeof':'.sizeof(VALUE)', 'max':'.max(1,2)', 'min':'.min(1,2)',
 'lobyte':'.lobyte($1234)', 'hibyte':'.hibyte($1234)',
 'bankbyte':'.bankbyte($123456)',
 'constructor_concat':'.concat("A")', 'constructor_string':'.string(VALUE)',
 'constructor_ident':'.ident("VALUE")',
 'constructor_sprintf':'.sprintf("%d",.const(VALUE))',
 'constructor_left':'.left(1,{VALUE})',
}
def cases():
 out={}
 for dialect in ('ca65','65816','huc6280'):
  for name,call in CALLS.items():
   empty.add_contexts(out,dialect,name,'VALUE=7\n.byte .tcount({'+call+'})\n')
 assert len(out)==330;return out
if __name__=='__main__':
 capture=load('capture',ROOT/'docs/plans/2026-10-19-ca65-scope-binding/probe.py')
 prepared=cases();capture.cases=lambda:prepared;capture.FILES=empty.FILES;capture.main()
 path=Path(sys.argv[1]);facts=json.loads(path.read_text());assert len(facts['cases'])==330
 for case in facts['cases']:case['dialect']=case['name'].split('_',1)[0]
 path.write_text(json.dumps(facts,indent=2)+'\n')
