"""Remove only dead-process etcd fixture data inside this phase's owned slot."""
from pathlib import Path
import os, re, subprocess, time, json, shutil
slot=Path(os.environ['CARGO_TARGET_DIR'])
commands=subprocess.check_output(['ps','-axo','command='],text=True)
removed=[]
for path in slot.glob('etcd-contract-*'):
    match=re.fullmatch(r'etcd-contract-(\d+)-\d+',path.name)
    if not match or not path.is_dir() or time.time()-path.stat().st_mtime<1800: continue
    pid=int(match[1])
    try: os.kill(pid,0)
    except ProcessLookupError: pass
    else: continue
    if str(path) in commands or path.name in commands: continue
    size=sum(p.stat().st_size for p in path.rglob('*') if p.is_file())
    removed.append(dict(path=str(path),holder_pid=pid,holder_absent=True,not_referenced_by_process=True,bytes=size))
    shutil.rmtree(path)
Path(__file__).with_name('fixture-cleanup-items.json').write_text(json.dumps(removed,indent=2)+'\n')
print(json.dumps(dict(removed=len(removed),bytes=sum(r['bytes'] for r in removed))))
