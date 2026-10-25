"""Verify protected Go/module files, readonly plans, and restored behavior mutations."""
from pathlib import Path
import hashlib,json,os,subprocess
out=Path('docs/rust/schedule-execution/logs/task_028')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
before=json.loads((out/'readonly-before.json').read_text())
paths=[]
for directory,dirs,files in os.walk('.'):
 dirs[:]=[d for d in dirs if d not in ['.git','target']]
 for f in files:
  p=Path(directory)/f
  if p.suffix=='.go' or p.name in ['go.mod','go.sum','go.work','go.work.sum'] or str(p) in before:
   paths.append(p)
after={str(p):sha(p) for p in paths}
changes={p:[before.get(p),after.get(p)] for p in before.keys()|after.keys() if before.get(p)!=after.get(p)}
status=subprocess.check_output(['git','status','--porcelain=v1','-z'])
protected_status=[s.decode() for s in status.split(b'\0') if s and (Path(s[3:].decode()).suffix=='.go' or Path(s[3:].decode()).name in ['go.mod','go.sum','go.work','go.work.sum'])]
red=json.loads((out/'red.json').read_text())
restored=all(sha(Path(p))==v for p,v in red['production_sha256'].items())
result=dict(readonly_files=len(before),go_module_files=sum(Path(p).name!='plan.md' for p in before),readonly_changes=changes,protected_git_status=protected_status,mutation_restored=restored,diff_check_exit_code=subprocess.run(['git','diff','--check']).returncode,fixture_sha256=sha(Path('docs/rust/schedule-execution/fixtures/task_028.json')))
(out/'verification.json').write_text(json.dumps(result,indent=2)+'\n');print(result)
assert not changes and not protected_status and restored and result['diff_check_exit_code']==0
