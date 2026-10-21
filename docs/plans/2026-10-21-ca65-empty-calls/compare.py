"""Compare exact ca65-family exit codes, native bytes and refused outputs."""
import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

def main():
 parser=argparse.ArgumentParser();parser.add_argument('binary',type=Path);parser.add_argument('facts',type=Path);parser.add_argument('output',type=Path);args=parser.parse_args();binary=args.binary.resolve(strict=True);facts=json.loads(args.facts.read_text());rows=[]
 with tempfile.TemporaryDirectory(prefix='asm198x-empty-call-compare-') as td:
  root=Path(td)
  for name,text in facts['files'].items():(root/name).write_text(text)
  for case in facts['cases']:
   assert case['native'][-1]['exit']!=124
   (root/'input.s').write_text(case['source']);(root/'input.cfg').write_text(case['config']);(root/'out.bin').write_bytes(b'previous image');dialect=case['dialect']
   argv=[str(binary),'--dialect',dialect,*(['-C','input.cfg'] if dialect=='ca65' else []),'input.s','-o','out.bin']
   p=subprocess.run(argv,cwd=root,capture_output=True,text=True,timeout=5,check=False);data=(root/'out.bin').read_bytes();accepted=case['native'][-1]['exit']==0
   matches=p.returncode==(0 if accepted else 1) and (data.hex()==case['native_hex'] if accepted else data==b'previous image')
   rows.append(dict(name=case['name'],argv=argv,exit=p.returncode,stdout=p.stdout,stderr=p.stderr,hex=data.hex(),matches_native=matches))
 args.output.write_text(json.dumps(dict(binary=str(binary),sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),results=rows),indent=2)+'\n');print('cases',len(rows),'differences',sum(not r['matches_native'] for r in rows))
if __name__=='__main__':main()
