import gzip,hashlib,json,os,shutil,stat,subprocess,sys
from pathlib import Path
repo=Path.cwd();out=repo/'docs/plans/2026-10-21-ca65-linked-faraddr/cleanup-925';out.mkdir(exist_ok=True)
keep={'asm198x-faraddr-producer','asm198x-faraddr-local-package'}
def digest(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  for b in iter(lambda:f.read(1024*1024),b''):h.update(b)
 return h.hexdigest()
def scan(root):
 result=[]
 for p in [root,*sorted(root.rglob('*'))] if root.is_dir() else [root]:
  assert p.name!='.git',str(p)
  s=p.lstat();r=dict(path=str(p),mode=s.st_mode,size=s.st_size,inode=s.st_ino,device=s.st_dev,links=s.st_nlink,mtime_ns=s.st_mtime_ns)
  if stat.S_ISLNK(s.st_mode):r['target']=os.readlink(p)
  elif stat.S_ISREG(s.st_mode):r['sha256']=digest(p)
  else:assert stat.S_ISDIR(s.st_mode),str(p)
  result.append(r)
 return result
if sys.argv[1]=='retain':
 roots=sorted([p for p in Path('/private/tmp').glob('asm198x-faraddr-*') if p.name not in keep]+list(Path('/private/tmp').glob('asm198x-canonical-reader-*'))+[Path('/private/tmp')/n for n in ['asm198x-data-context-v3-producer','asm198x-data-context-v3-local-package','asm198x-audit-faraddr-candidates.py','asm198x-rejected-reader-cleanup.py'] if (Path('/private/tmp')/n).exists()])
 assert len(roots)==len(set(roots))
 assert digest(Path('/private/tmp/asm198x-data-context-v3-producer/asm198x'))=='b95498886cb76841ff4831c9018eaefb1efc859b3e86bf3fb70f7722bc9f9f43'
 assert digest(Path('/private/tmp/asm198x-data-context-v3-local-package/asm198x-aarch64-apple-darwin.tar.xz'))=='35d835cf5bb5309993383a96af4e33061be341d8646afa91b37598c1e8f01348'
 assert digest(Path('/private/tmp/asm198x-faraddr-producer/asm198x'))=='731d26ee912455c7bdaf39a3529f72ba573e4af7da8121e37046c936a714a2af'
 known={}
 q=repo/'docs/plans/2026-10-21-ca65-linked-faraddr/qualification-925'
 for r in json.loads((q/'manifest.json').read_text()):
  p=q/r['path'];assert digest(p)==r['stored_sha256'];assert hashlib.sha256(gzip.decompress(p.read_bytes())).hexdigest()==r['sha256'];known[r['sha256']]=str(p.relative_to(repo))
 records=[];retained=[]
 for root in roots:
  rows=scan(root);records.extend(rows)
  for r in rows:
   if 'sha256' not in r:continue
   p=Path(r['path']);sha=r['sha256']
   if sha=='b95498886cb76841ff4831c9018eaefb1efc859b3e86bf3fb70f7722bc9f9f43' or p.name.endswith(('.tar.xz','.zip')):
    r['disposition']='Disposable compiled artifact; complete source/lock/executable/archive identity and checked predecessor-control evidence retained.';continue
   if sha not in known:
    dest=out/'blobs'/(sha+'.gz');dest.parent.mkdir(exist_ok=True);dest.write_bytes(gzip.compress(p.read_bytes(),mtime=0));assert dest.stat().st_size<45*1024*1024;known[sha]=str(dest.relative_to(repo))
   retained.append(dict(source=str(p),sha256=sha,retained=known[sha]))
 data=dict(roots=[str(p) for p in roots],records=records,retained=retained)
 (out/'preflight-full.json.gz').write_bytes(gzip.compress(json.dumps(data).encode(),mtime=0))
 summary=dict(roots=len(roots),regular_files=sum('sha256'in r for r in records),logical_bytes=sum(r['size'] for r in records if 'sha256'in r),df_before=subprocess.check_output(['df','-k','/private/tmp'],text=True))
 (out/'preflight.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary))
else:
 assert sys.argv[1]=='remove'
 data=json.loads(gzip.decompress((out/'preflight-full.json.gz').read_bytes()));expected={r['path']:{k:v for k,v in r.items() if k!='disposition'} for r in data['records']}
 actual=[r for p in data['roots'] for r in scan(Path(p))];assert {r['path']:r for r in actual}==expected
 for r in data['retained']:assert hashlib.sha256(gzip.decompress((repo/r['retained']).read_bytes())).hexdigest()==r['sha256']
 head=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();remote=subprocess.check_output(['git','ls-remote','origin','refs/heads/audit/ca65-linked-faraddr-925'],text=True).split()[0];assert head==remote
 assert subprocess.check_output(['git','ls-remote','origin','refs/heads/main'],text=True).split()[0]=='ee522abd40a0bfa7a61b5581679679690717ccff'
 processes=subprocess.check_output(['ps','-axo','pid,command'],text=True)
 active=[line for line in processes.splitlines() if any(p in line for p in data['roots']) and int(line.strip().split()[0])!=os.getpid()];assert not active,active
 for name in data['roots']:
  p=Path(name)
  if p.is_dir():shutil.rmtree(p)
  else:p.unlink()
 assert not any(Path(p).exists() for p in data['roots'])
 result=dict(removed_roots=len(data['roots']),removed_regular_files=len(data['retained']),df_after=subprocess.check_output(['df','-k','/private/tmp'],text=True),retained=['/private/tmp/asm198x-faraddr-producer','/private/tmp/asm198x-faraddr-local-package','/private/tmp/asm198x-scalar-build','/private/tmp/asm198x-scalar-release-build'],reason='Checked #925 baseline and actively reused caches for the approved canonical reader; remove after a checked successor replaces the baseline or reader work finishes.')
 (out/'completed.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
