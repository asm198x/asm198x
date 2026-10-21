"""Native zero-argument function admission, including lazy/captured source."""
import importlib.util
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
FILES={}
FUNCTIONS=['concat','string','ident','sprintf','strlen','strat','left','right','mid','blank','tcount','match','xmatch','ismnem','ismnemonic','const','sizeof']
def cases():
 out={}
 for function in FUNCTIONS:
  body=f'.byte .{function}()\n'
  for context in ['direct','included','macro','inactive','unused']:
   key=function+'_'+context
   if context=='included':FILES[key+'.inc']=body;source=f'.include "{key}.inc"\n'
   elif context in ['macro','unused']:source='.macro run\n'+body+'.endmacro\n'+('run\n' if context=='macro' else '.byte 9\n')
   elif context=='inactive':source='.if 0\n'+body+'.endif\n.byte 9\n'
   else:source=body
   out[key]=source
 assert len(out)==85
 return out
if __name__=='__main__':
 spec=importlib.util.spec_from_file_location('capture',ROOT/'docs/plans/2026-10-19-ca65-scope-binding/probe.py');assert spec and spec.loader
 capture=importlib.util.module_from_spec(spec);spec.loader.exec_module(capture)
 prepared=cases();capture.cases=lambda:prepared;capture.FILES=FILES;capture.main()
