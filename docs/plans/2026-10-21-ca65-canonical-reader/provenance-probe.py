"""Capture ca65 physical/generated register-name admission across consumers."""
import importlib.util
from pathlib import Path

ROOT=Path(__file__).resolve().parents[3]
FILES={}
FORMS={'physical':'A','constructed':'.ident("A")','selected_physical':'.left(1,{A})','selected_constructed':'.left(1,{.ident("A")})','right_constructed':'.right(1,{IGNORE,.ident("A")})'}
CONSUMERS={'data':'.byte {token}\n','query':'.byte .defined({token}),.ref({token})\n','repeat':'.repeat 1,{token}\n.byte 2\n.endrepeat\n','export':'.export {token}\n.byte 7\n'}

def cases():
 out={}
 for form,token in FORMS.items():
  for consumer,template in CONSUMERS.items():
   body='.ident("A") = 2\n'+template.replace('{token}',token)
   for context in ['direct','included','macro','inactive','unused']:
    key=form+'_'+consumer+'_'+context
    if context=='included':FILES[key+'.inc']=body;source=f'.include "{key}.inc"\n'
    elif context in ['macro','unused']:source='.macro run\n'+body+'.endmacro\n'+('run\n' if context=='macro' else '.byte 9\n')
    elif context=='inactive':source='.if 0\n'+body+'.endif\n.byte 9\n'
    else:source=body
    out[key]=source
 assert len(out)==100
 return out

if __name__=='__main__':
 spec=importlib.util.spec_from_file_location('capture',ROOT/'docs/plans/2026-10-19-ca65-scope-binding/probe.py');assert spec and spec.loader
 capture=importlib.util.module_from_spec(spec);spec.loader.exec_module(capture)
 prepared=cases();capture.cases=lambda:prepared;capture.FILES=FILES;capture.main()
