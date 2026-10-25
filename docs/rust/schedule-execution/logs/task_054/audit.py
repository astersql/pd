"""Read-only file-list, per-file hash and scope audit for task054."""
from pathlib import Path
import subprocess,json,hashlib,re
root=Path(__file__).resolve().parents[5]
out=Path(__file__).resolve().parent
before=json.loads(Path('/tmp/pd-task054-go-before.json').read_text())
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=root).decode().split('\0')
after={x:hashlib.sha256((root/x).read_bytes()).hexdigest() for x in paths if x and (x.endswith('.go') or Path(x).name in ['go.mod','go.sum','go.work','go.work.sum']) and (root/x).is_file()}
changes=[x for x in before.keys()|after.keys() if before.get(x)!=after.get(x)]
status=subprocess.check_output(['git','status','--porcelain','-z'],cwd=root).decode().split('\0')
go_status=[s for s in status if s and (s[3:].endswith('.go') or Path(s[3:]).name in ['go.mod','go.sum','go.work','go.work.sum'])]
plans=['.plans/2026-10-25-调度执行内核/plan.md','.plans/2026-10-25-调度执行内核/05-检查器/plan.md']
plan_unchanged={p:subprocess.check_output(['git','show',f'HEAD:{p}'],cwd=root)==(root/p).read_bytes() for p in plans}
docs=['docs/rust/schedule-execution/task-054.md','pkg/schedule/checker/split_scatter_retry.rs.md']
broken=[]
for p in docs:
 for link in re.findall(r'\]\(([^)]+)\)',(root/p).read_text()):
  if not '://' in link and not (root/p).parent.joinpath(link.split('#')[0]).exists():broken.append([p,link])
diff=subprocess.run(['git','diff','--check'],cwd=root,capture_output=True,text=True)
result=dict(go_files=len(after),go_file_list_and_hashes_equal=not changes,go_changes=changes,go_status=go_status,readonly_plans_unchanged=plan_unchanged,broken_links=broken,diff_check_exit=diff.returncode,diff_check_output=diff.stdout)
(out/'audit.json').write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n')
print(json.dumps(result,ensure_ascii=False));assert not changes and not go_status and all(plan_unchanged.values()) and not broken and diff.returncode==0
