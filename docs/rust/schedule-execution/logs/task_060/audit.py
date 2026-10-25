from pathlib import Path
import os,json,hashlib,subprocess,re,tomllib
root=Path(__file__).resolve().parents[5]
os.chdir(root)
out=Path(__file__).parent
before=json.loads((out/'go-before.json').read_text())
after={}
for directory,dirs,files in os.walk('.'):
 dirs[:]=[d for d in dirs if d not in ('.git','target')]
 for name in files:
  p=Path(directory)/name
  if p.suffix=='.go' or p.name in ('go.mod','go.sum','go.work','go.work.sum'):
   after[str(p)]=hashlib.sha256(p.read_bytes()).hexdigest()
assert before==after, 'Go/module file manifest or contents changed'
status=subprocess.check_output(['git','status','--porcelain','-z','--untracked-files=all']).decode().split('\0')
go_status=[s for s in status if s and (Path(s[3:]).suffix=='.go' or Path(s[3:]).name in ('go.mod','go.sum','go.work','go.work.sum'))]
assert not go_status,go_status
plans=['.plans/2026-10-25-调度执行内核/plan.md','.plans/2026-10-25-调度执行内核/06-区域散射/plan.md']
for p in plans:
 assert Path(p).read_bytes()==subprocess.check_output(['git','show','HEAD:'+p]),p
subprocess.run(['git','diff','--check'],check=True)
for p in [root/'pkg/schedule/scatter/scatter_operator.rs.md',root/'docs/rust/schedule-execution/task-060.md']:
 if p.exists():
  for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
   target=p.parent/link.split('#')[0]
   if target.resolve()==(out/'audit.json').resolve(): continue # This successful audit creates its own result.
   assert target.exists(),(p,link)
def remote_packages(text):
 return [p for p in tomllib.loads(text)['package'] if 'source' in p]
assert remote_packages(Path('Cargo.lock').read_text())==remote_packages(subprocess.check_output(['git','show','HEAD:Cargo.lock']).decode())
result=dict(remote_lock_entries_unchanged=True,go_files=len(before),go_hashes_unchanged=True,go_status=go_status,readonly_plans_unchanged=True,diff_check_exit=0,links_valid=True)
(out/'audit.json').write_text(json.dumps(result,indent=2)+'\n')
print(result)
