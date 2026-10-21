"""Native canonical-reader acceptance controls; no reader implementation."""
import importlib.util
from pathlib import Path

ROOT=Path(__file__).resolve().parents[3]
FILES={'payload.bin':'ABCDEFGH'}
BODIES={
 'res_right':'.res .right(3,{9,1,2})+1\n',
 'res_mid':'.res .mid(2,3,{9,1,2,8})+1\n',
 'res_nested':'.res .left(3,{.right(3,{9,1,2})})+1\n',
 'res_empty':'.res .left(0,{1,2})\n',
 'res_trailing':'.res .left(2,{1,2})+1\n',
 'res_overflow':'.res .left(3,{1,$ff})+1\n',
 'res_prior_read':'VALUE=2\n.res .left(3,{VALUE,2})+.ref(VALUE)\n.byte .ref(VALUE)\n',
 'res_suffix_const':'VALUE=2\n.res .left(3,{1,2})+.const(VALUE)\n.byte .ref(VALUE)\n',
 'res_selector_count':'VALUE=2\n.res .left(.const(VALUE)+2,{1,2})\n.byte .ref(VALUE)\n',
 'repeat_right':'.repeat .right(3,{0,2,INDEX})\n.byte INDEX\n.endrepeat\n',
 'repeat_mid':'.repeat .mid(2,3,{0,2,INDEX,9})\n.byte INDEX\n.endrepeat\n',
 'repeat_empty':'.repeat .left(0,{1,INDEX})\n.byte 7\n.endrepeat\n',
 'align_right':'.byte 7\n.align .right(3,{9,2,3})+1\n',
 'align_mid':'.byte 7\n.align .mid(2,3,{9,2,3,8})\n',
 'assert_right':'.assert .right(3,{0,1,error}),"ok"\n.byte 9\n',
 'assert_mid':'.assert .mid(2,3,{0,1,error,9}),"ok"\n.byte 9\n',
 'incbin_all':'.incbin .left(5,{"payload.bin",1,2})\n',
 'incbin_name_offset':'.incbin .left(3,{"payload.bin",1}),2\n',
 'incbin_right':'.incbin "payload.bin",.right(3,{0,1,2})+1\n',
 'scoped_read':'VALUE=2\n.scope INNER\n.res .left(3,{VALUE,2})+.ref(VALUE)\n.byte .ref(VALUE)\n.endscope\n',
}

def cases():
 out={}
 for name,body in BODIES.items():
  for context in ['direct','included','macro','inactive','unused']:
   key=name+'_'+context
   if context=='included':FILES[key+'.inc']=body;source=f'.include "{key}.inc"\n'
   elif context in ['macro','unused']:source='.macro run\n'+body+'.endmacro\n'+('run\n' if context=='macro' else '.byte 9\n')
   elif context=='inactive':source='.if 0\n'+body+'.endif\n.byte 9\n'
   else:source=body
   out[key]=source
 assert len(out)==100
 return out

if __name__=='__main__':
 spec=importlib.util.spec_from_file_location('capture',ROOT/'docs/plans/2026-10-19-ca65-scope-binding/probe.py')
 assert spec and spec.loader
 capture=importlib.util.module_from_spec(spec);spec.loader.exec_module(capture)
 prepared=cases();capture.cases=lambda:prepared;capture.FILES=FILES;capture.main()
