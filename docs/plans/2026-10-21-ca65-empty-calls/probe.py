"""Retain native empty-call and valid-counterpart rules for the ca65 family."""
import importlib.util
import json
import sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
FILES={}
FUNCTIONS=['concat','string','ident','sprintf','strlen','strat','left','right','mid','blank','tcount','match','xmatch','ismnem','ismnemonic','const','sizeof']
VALID={'concat':'.concat("A")','string':'.string(VALUE)','ident':'.ident("VALUE")','sprintf':'.sprintf("%d",7)','strlen':'.strlen("A")','strat':'.strat("A",0)','left':'.left(1,{7})','right':'.right(1,{7})','mid':'.mid(0,1,{7})','blank':'.blank({})','tcount':'.tcount({})','match':'.match({},{})','xmatch':'.xmatch({},{})','ismnem':'.ismnem(lda)','ismnemonic':'.ismnemonic(lda)','const':'.const(VALUE)'}

NESTED={
 'blank_match':'.byte .match({.blank()},{1})\n',
 'tcount_match':'.byte .match({.tcount()},{0})\n',
 'blank_tokens':'.byte .tcount({.blank()})\n',
 'const_tokens':'VALUE=7\n.byte .tcount({.const(VALUE)})\n',
 'concat_tokens':'.byte .tcount({.concat("A")})\n',
 'sprintf_numeric':'VALUE=7\n.byte .tcount({.sprintf("%d",.const(VALUE))})\n',
}

def add_contexts(out,dialect,name,body):
 for context in ['direct','included','macro','inactive','unused']:
  key='_'.join([dialect,name,context])
  if context=='included':FILES[key+'.inc']=body;source=f'.include "{key}.inc"\n'
  elif context in ['macro','unused']:source='.macro run\n'+body+'.endmacro\n'+('run\n' if context=='macro' else '.byte 9\n')
  elif context=='inactive':source='.if 0\n'+body+'.endif\n.byte 9\n'
  else:source=body
  out[key]=('' if dialect=='ca65' else f'.setcpu "{dialect}"\n')+source

def cases():
 out={}
 for dialect in ['ca65','65816','huc6280']:
  for function in FUNCTIONS:
   for style in ['empty','spaced','valid']:
    if style=='valid':
     if function not in VALID:continue
     body='VALUE=7\n.byte '+VALID[function]+'\n'
    else:body=f'.byte .{function}'+('()' if style=='empty' else ' ( \t )')+'\n'
    add_contexts(out,dialect,function+'_'+style,body)
  for name,body in NESTED.items():add_contexts(out,dialect,'nested_'+name,body)
 assert len(out)==840
 return out

if __name__=='__main__':
 spec=importlib.util.spec_from_file_location('capture',ROOT/'docs/plans/2026-10-19-ca65-scope-binding/probe.py');assert spec and spec.loader
 capture=importlib.util.module_from_spec(spec);spec.loader.exec_module(capture)
 prepared=cases();capture.cases=lambda:prepared;capture.FILES=FILES;capture.main()
 path=Path(sys.argv[1]);facts=json.loads(path.read_text());assert len(facts['cases'])==840
 for case in facts['cases']:case['dialect']=case['name'].split('_',1)[0]
 path.write_text(json.dumps(facts,indent=2)+'\n')
