from pathlib import Path
import os,json,hashlib,subprocess
root=Path.cwd(); log=root/'docs/rust/schedule-execution/logs/task_094'
before=json.loads((log/'before.json').read_text()); after={}
for base,dirs,files in os.walk(root):
 dirs[:]=[d for d in dirs if d not in ('.git','target')]
 for name in files:
  p=Path(base)/name
  if p.suffix=='.go' or name in ('go.mod','go.sum','go.work','go.work.sum','plan.md'):
   after[str(p.relative_to(root))]=hashlib.sha256(p.read_bytes()).hexdigest()
changed=[p for p in before.keys()|after.keys() if before.get(p)!=after.get(p)]
status=subprocess.check_output(['git','status','--porcelain','--untracked-files=all']).decode()
go_status=[s for s in status.splitlines() if s.endswith('.go') or any(s.endswith(x) for x in ('go.mod','go.sum','go.work','go.work.sum'))]
check=subprocess.run(['git','diff','--check'],capture_output=True,text=True)
report=dict(files=len(after),hash_changes=changed,go_status=go_status,diff_check_exit=check.returncode,diff_check_output=check.stdout+check.stderr)
(log/'audit.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report)); assert not changed and not go_status and check.returncode==0
