"""Remove only generated fixture artifacts whose recorded PID no longer exists."""
from pathlib import Path
import os,re,shutil,json
slot=Path(os.environ['CARGO_TARGET_DIR'])
removed=[];total=0
for p in sorted(slot.iterdir()):
 m=re.fullmatch(r'(?:etcd-contract-(\d+)-\d+|rust-etcd-fixture-(\d+))',p.name)
 if not m: continue
 pid=int(m[1] or m[2])
 try: os.kill(pid,0)
 except ProcessLookupError: pass
 except PermissionError: continue
 else: continue
 # Require a real generated runtime data dir or a regular generated helper binary.
 if p.is_symlink(): continue
 size=sum(f.stat().st_size for f in p.rglob('*') if f.is_file()) if p.is_dir() else p.stat().st_size
 if p.is_dir(): shutil.rmtree(p)
 else: p.unlink()
 removed.append({'name':p.name,'pid_not_running':pid,'bytes':size});total+=size
 if total >= 3*1024**3: break
Path(__file__).with_name('fixture-cleanup.json').write_text(json.dumps({'slot':str(slot),'removed':removed,'bytes':total},indent=2)+'\n')
print('removed inactive fixture runtime data/helpers:',len(removed),'bytes:',total)
