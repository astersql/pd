"""Reclaim dead-process test fixtures, preserving Cargo's debug build cache."""
from pathlib import Path
import os,re,subprocess,time,json,shutil
slot=Path(os.environ['CARGO_TARGET_DIR'])
commands=subprocess.check_output(['ps','-axo','command='],text=True)
removed=[]
def dead(pid):
 try: os.kill(pid,0)
 except ProcessLookupError: return True
 return False
for path in slot.glob('etcd-contract-*'):
 match=re.fullmatch(r'etcd-contract-(\d+)-\d+',path.name)
 if not match or not path.is_dir() or not dead(int(match[1])):continue
 if str(path) in commands or path.name in commands:continue
 size=sum(p.stat().st_size for p in path.rglob('*') if p.is_file())
 removed.append(dict(path=str(path),kind='fixture-data',holder_pid=int(match[1]),holder_absent=True,not_referenced_by_process=True,bytes=size))
 shutil.rmtree(path)
count=0
for path in sorted(slot.glob('rust-etcd-fixture-*'),key=lambda p:p.stat().st_mtime):
 match=re.fullmatch(r'rust-etcd-fixture-(\d+)',path.name)
 if not match or not path.is_file() or path.is_symlink() or time.time()-path.stat().st_mtime<21600:continue
 if not dead(int(match[1])) or str(path) in commands or path.name in commands:continue
 removed.append(dict(path=str(path),kind='per-process-fixture-executable',holder_pid=int(match[1]),holder_absent=True,not_referenced_by_process=True,bytes=path.stat().st_size))
 path.unlink();count+=1
 if count==100:break
Path(__file__).with_name('fixture-cleanup-retry-items.json').write_text(json.dumps(removed,indent=2)+'\n')
print(json.dumps(dict(removed=len(removed),bytes=sum(r['bytes'] for r in removed))))
