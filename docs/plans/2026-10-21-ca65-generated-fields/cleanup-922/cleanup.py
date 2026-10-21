import gzip
import hashlib
import json
import os
import shutil
import stat
import subprocess
import sys
from pathlib import Path

repo = Path('/Users/stevehill/Projects/198x/Asm198x/asm198x-ca65-text-consumers')
out = repo / 'docs/plans/2026-10-21-ca65-generated-fields/cleanup-922'
out.mkdir(exist_ok=True)
excluded = {'asm198x-scalar-build', 'asm198x-scalar-release-build'}
extra = [Path('/private/tmp') / name for name in ['asm198x-file-consumers-audit', 'asm198x-repeat-name-audit', 'asm198x-generated-fields-audit', 'asm198x-audit-scalar-candidates.py']]
extra.append(Path('/var/folders/94/p_ng_dqx7tq9rx2rcw0y8zth0000gn/T/asm198x-candidate-niXEea'))

def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()

def scan(root):
    paths = [root, *sorted(root.rglob('*'))] if root.is_dir() else [root]
    records = []
    for path in paths:
        st = path.lstat()
        assert path.name != '.git', str(path)
        row = dict(path=str(path), mode=st.st_mode, size=st.st_size, inode=st.st_ino, device=st.st_dev, links=st.st_nlink, mtime_ns=st.st_mtime_ns)
        if stat.S_ISLNK(st.st_mode): row['target'] = os.readlink(path)
        elif stat.S_ISREG(st.st_mode): row['sha256'] = digest(path)
        else: assert stat.S_ISDIR(st.st_mode), str(path)
        records.append(row)
    return records

if sys.argv[1] == 'retain':
    roots = sorted([p for p in Path('/private/tmp').glob('asm198x-scalar*') if p.name not in excluded] + [p for p in extra if p.exists()])
    assert len(roots) == len(set(roots))
    existing = {}
    q = repo / 'docs/plans/2026-10-21-ca65-scalar-consumers/qualification-922'
    for entry in json.loads((q / 'manifest.json').read_text()):
        target = q / entry['path']
        assert digest(target) == entry['stored_sha256']
        assert hashlib.sha256(gzip.decompress(target.read_bytes())).hexdigest() == entry['sha256']
        existing[entry['sha256']] = str(target.relative_to(repo))
    records = []
    retained = []
    compiled = {'031609caef87d474137aadb56695c160d65bd4b0360cfc2f20474e86b30b61c4', '76066fe8048d203a7c567b43953d0fce193db8c756254617c1f71294575c4449'}
    for root in roots:
        rows = scan(root)
        records.extend(rows)
        for row in rows:
            if 'sha256' not in row: continue
            path = Path(row['path'])
            if row['sha256'] in compiled or path.name.endswith(('.tar.xz', '.zip')):
                row['disposition'] = 'disposable compiled artifact; source, lock, executable/archive hashes and full process evidence retained'
                continue
            key = row['sha256']
            if key not in existing:
                target = out / 'blobs' / (key + '.gz')
                target.parent.mkdir(exist_ok=True)
                with path.open('rb') as source, gzip.open(target, 'wb') as dest: shutil.copyfileobj(source, dest)
                assert target.stat().st_size < 45 * 1024 * 1024, str(target)
                assert hashlib.sha256(gzip.decompress(target.read_bytes())).hexdigest() == key
                existing[key] = str(target.relative_to(repo))
            retained.append(dict(source=str(path),sha256=key,retained=existing[key]))
    with gzip.open(out / 'preflight-full.json.gz', 'wt') as f: json.dump(dict(roots=[str(p) for p in roots],records=records,retained=retained),f)
    summary = dict(roots=len(roots),files=sum('sha256' in x for x in records),logical_bytes=sum(x['size'] for x in records if 'sha256' in x),retained_data_files=len(retained),excluded_caches=sorted(excluded),df_before=subprocess.run(['df','-k','/private/tmp'],check=True,capture_output=True,text=True).stdout)
    (out / 'preflight.json').write_text(json.dumps(summary,indent=2)+'\n')
    print(json.dumps(summary,indent=2))
else:
    assert sys.argv[1] == 'remove'
    data=json.loads(gzip.decompress((out/'preflight-full.json.gz').read_bytes()))
    expected={x['path']:x for x in data['records']}
    for name in data['roots']:
        root=Path(name)
        for row in scan(root): assert row=={k:v for k,v in expected[row['path']].items() if k!='disposition'}, row['path']
    # Source refs and the complete compressed inventory must be pushed first.
    head=subprocess.run(['git','rev-parse','HEAD'],cwd=repo,check=True,capture_output=True,text=True).stdout.strip()
    remote=subprocess.run(['git','ls-remote','origin','refs/heads/audit/ca65-scalar-context-922'],cwd=repo,check=True,capture_output=True,text=True).stdout.split()[0]
    assert head==remote
    for name in data['roots']:
        path=Path(name)
        if path.is_dir(): shutil.rmtree(path)
        else: path.unlink()
    assert not any(Path(x).exists() for x in data['roots'])
    result=dict(removed_roots=len(data['roots']),removed_regular_files=sum('sha256' in x for x in data['records']),df_after=subprocess.run(['df','-k','/private/tmp'],check=True,capture_output=True,text=True).stdout,retained_caches=[str(Path('/private/tmp')/x) for x in sorted(excluded)],reason_caches_retained='Actively reused by independent #921 in the same task worktree; disposable after its checked replacement is retained.')
    (out/'completed.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
