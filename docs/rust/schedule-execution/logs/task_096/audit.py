from pathlib import Path
import os,json,hashlib,subprocess,gzip
root=Path.cwd()
logs=root/'docs/rust/schedule-execution/logs/task_096'
before=json.loads(gzip.decompress((logs/'protected-hashes.json.gz').read_bytes()))
after={}
for parent,dirs,files in os.walk(root):
 dirs[:]=[d for d in dirs if d not in ['.git','target']]
 for name in files:
  if name.endswith('.go') or name in ['go.mod','go.sum','go.work','go.work.sum','plan.md']:
   p=Path(parent)/name
   after[str(p.relative_to(root))]=hashlib.sha256(p.read_bytes()).hexdigest()
changes=sorted(p for p in before.keys()|after.keys() if before.get(p)!=after.get(p))
status=subprocess.check_output(['git','status','--short','--','*.go','go.mod','go.sum','go.work','go.work.sum','**/go.mod','**/go.sum','**/go.work','**/go.work.sum'],text=True).splitlines()
r=subprocess.run(['git','diff','--check'],capture_output=True,text=True)
audit=dict(files=len(before),hash_changes=changes,go_status=status,diff_check_exit=r.returncode,diff_check_output=r.stdout+r.stderr)
(logs/'audit.json').write_text(json.dumps(audit,indent=2)+'\n')
print(json.dumps(audit))
assert not changes and not status and not r.returncode
