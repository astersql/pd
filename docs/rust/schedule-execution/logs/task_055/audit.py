"""Compare repository Go/module content with the pre-task snapshot."""
from pathlib import Path
import hashlib,json,os,subprocess,re
root=Path(__file__).resolve().parents[5]
os.chdir(root)
before=json.loads(Path('/tmp/pd-task055-go-before.json').read_text())
after={}
for parent,dirs,files in os.walk('.'):
 dirs[:]=[d for d in dirs if d not in ['.git','target']]
 for name in files:
  p=Path(parent)/name
  if p.suffix=='.go' or name in ['go.mod','go.sum','go.work','go.work.sum']:
   after[str(p)]=hashlib.sha256(p.read_bytes()).hexdigest()
changed=[p for p in before.keys()|after.keys() if before.get(p)!=after.get(p)]
plans=['.plans/2026-10-25-调度执行内核/plan.md','.plans/2026-10-25-调度执行内核/05-检查器/plan.md']
plan_checks={p:Path(p).read_bytes()==subprocess.check_output(['git','show','HEAD:'+p]) for p in plans}
go_status=subprocess.check_output(['git','status','--porcelain','--untracked-files=all','--','*.go','go.mod','go.sum','go.work','go.work.sum'],text=True)
links={}
for doc in [Path('docs/rust/schedule-execution/task-055.md'),Path('pkg/schedule/checker/checker_dispatch.rs.md')]:
 for target in re.findall(r'\]\(([^)]+)\)',doc.read_text()):
  if '://' not in target: links[str(doc)+':'+target]=(doc.parent/target.split('#')[0]).exists()
result=dict(go_file_count=len(after),go_contents_unchanged=not changed,changed_go_files=changed,go_git_status=go_status,readonly_plans_match_head=plan_checks,links=links,fixture_sha256=hashlib.sha256(Path('docs/rust/schedule-execution/fixtures/task_055.json').read_bytes()).hexdigest())
Path(__file__).with_name('audit.json').write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n')
assert not changed and not go_status and all(plan_checks.values()) and all(links.values()),result
print(json.dumps(result,ensure_ascii=False))
