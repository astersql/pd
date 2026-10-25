"""Verify the protected Go/module tree, read-only plans, and evidence links."""
from pathlib import Path
import os,hashlib,json,subprocess,re
root=Path(__file__).resolve().parents[5];out=Path(__file__).resolve().parent
actual={}
for directory,dirs,files in os.walk(root):
    dirs[:]=[d for d in dirs if d not in ['.git','target']]
    for name in files:
        if name.endswith('.go') or name in ['go.mod','go.sum','go.work','go.work.sum']:
            p=Path(directory)/name
            actual[str(p.relative_to(root))]=hashlib.sha256(p.read_bytes()).hexdigest()
before=json.loads((out/'go-before.json').read_text())
assert actual==before, 'protected file list or contents changed'
plans=['.plans/2026-10-25-调度执行内核/plan.md','.plans/2026-10-25-调度执行内核/05-检查器/plan.md']
for p in plans:assert (root/p).read_bytes()==subprocess.check_output(['git','show','HEAD:'+p],cwd=root),p
sources=['pkg/schedule/checker/patrol.rs.md','docs/rust/schedule-execution/task-056.md']
links=0
for name in sources:
    path=root/name
    for target in re.findall(r'\]\(([^)]+)\)',path.read_text()):
        assert (path.parent/target.split('#')[0]).exists(),(name,target)
        links+=1
subprocess.run(['git','diff','--check'],cwd=root,check=True)
subprocess.run(['git','diff','--cached','--check'],cwd=root,check=True)
summary={'protected_go_module_files':len(actual),'protected_contents_and_list_unchanged':True,'go_git_status':subprocess.check_output(['git','status','--short','--','*.go','go.mod','go.sum','go.work','go.work.sum'],cwd=root,text=True),'readonly_plans_unchanged':plans,'relative_links_checked':links,'git_diff_check':0}
assert not summary['go_git_status']
(out/'audit.json').write_text(json.dumps(summary,indent=2,ensure_ascii=False)+'\n');print(json.dumps(summary,ensure_ascii=False))
