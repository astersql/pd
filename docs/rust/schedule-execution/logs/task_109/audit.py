from pathlib import Path
import os,json,hashlib,subprocess
logs=Path('docs/rust/schedule-execution/logs/task_109')
before=json.loads((logs/'readonly-before.json').read_text())
files=[]
for base,dirs,names in os.walk('.'):
 dirs[:]=[d for d in dirs if d not in ['.git','target']]
 for name in names:
  if name.endswith('.go') or name in ['go.mod','go.sum','go.work','go.work.sum']:
   files.append(str(Path(base)/name))
original=[p for p in before['hashes'] if not p.startswith('.plans/')]
assert set(files)==set(original), 'Go/module file list changed'
assert all(hashlib.sha256(Path(p).read_bytes()).hexdigest()==h for p,h in before['hashes'].items()), 'readonly hash changed'
status=subprocess.check_output(['git','status','--porcelain','--','*.go','go.mod','go.sum','go.work','go.work.sum']).decode()
assert status==before['status'],'readonly git status changed'
subprocess.run(['git','diff','--check'],check=True)
result=dict(go_and_module_files=len(files),readonly_plans=2,all_hashes_match=True,git_status_matches=True,diff_check_exit=0)
(logs/'readonly-check.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result))
