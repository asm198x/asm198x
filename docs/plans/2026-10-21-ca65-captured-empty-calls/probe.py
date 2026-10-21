"""Measure empty constructors versus numeric calls inside captured lists."""
import importlib.util
import json
import sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
def load(name,path):
 spec=importlib.util.spec_from_file_location(name,path);assert spec and spec.loader
 module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
empty=load('empty',ROOT/'docs/plans/2026-10-21-ca65-empty-calls/probe.py')
def cases():
 out={}
 for dialect in ('ca65','65816','huc6280'):
  for name in empty.FUNCTIONS:empty.add_contexts(out,dialect,name,'.byte .tcount({.'+name+'()})\n')
 assert len(out)==255;return out
if __name__=='__main__':
 capture=load('capture',ROOT/'docs/plans/2026-10-19-ca65-scope-binding/probe.py')
 prepared=cases();capture.cases=lambda:prepared;capture.FILES=empty.FILES;capture.main()
 path=Path(sys.argv[1]);facts=json.loads(path.read_text());assert len(facts['cases'])==255
 for case in facts['cases']:case['dialect']=case['name'].split('_',1)[0]
 path.write_text(json.dumps(facts,indent=2)+'\n')
