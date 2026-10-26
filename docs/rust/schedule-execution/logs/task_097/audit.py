from pathlib import Path
import gzip,json,hashlib,os,subprocess
root=Path.cwd();log=root/'docs/rust/schedule-execution/logs/task_097'
before=json.loads(gzip.decompress((log/'protected-hashes.json.gz').read_bytes()))
after={}
for base,dirs,files in os.walk(root):
 dirs[:]=[d for d in dirs if d not in ['.git','target']]
 for name in files:
  p=Path(base)/name
  if p.suffix=='.go' or name in ['go.mod','go.sum','go.work','go.work.sum','plan.md']:
   after[str(p.relative_to(root))]=hashlib.sha256(p.read_bytes()).hexdigest()
changed=[s for s in before.keys() | after.keys() if before.get(s)!=after.get(s)]
status=subprocess.check_output(['git','status','--porcelain','--untracked-files=all'],text=True)
go_status=[s for s in status.splitlines() if s.endswith(('.go','go.mod','go.sum','go.work','go.work.sum'))]
code=subprocess.run(['git','diff','--check']).returncode
record=dict(protected_files=len(before),end_files=len(after),changed=changed,go_status=go_status,diff_check_exit=code)
(log/'audit.json').write_text(json.dumps(record,indent=2)+'\n');print(record)
assert not changed and not go_status and code==0
