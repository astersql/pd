from pathlib import Path
import hashlib,json,subprocess,re
root=Path(__file__).resolve().parents[5]
def inventory():
 paths=subprocess.check_output(['rg','--files','--hidden','-g','!.git','-g','!target','-g','*.go','-g','go.mod','-g','go.sum','-g','go.work','-g','go.work.sum'],cwd=root).decode().splitlines()
 return {p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in paths}
before=json.loads(Path('/tmp/task053-go-before.json').read_text());after=inventory()
assert before==after
status=subprocess.check_output(['git','status','--porcelain','--','*.go','go.mod','go.sum','go.work','go.work.sum'],cwd=root)
assert status==Path('/tmp/task053-go-status').read_bytes()
plans=json.loads(Path('/tmp/task053-plans-before.json').read_text())
assert all(hashlib.sha256((root/p).read_bytes()).hexdigest()==h for p,h in plans.items())
links=[]
for name in ['docs/rust/schedule-execution/task-053.md','pkg/schedule/checker/split_scatter_observe.rs.md']:
 p=root/name
 for target in re.findall(r'\]\(([^)]+)\)',p.read_text()):
  assert (p.parent/target).exists(), target
  links.append(target)
subprocess.run(['git','diff','--check'],cwd=root,check=True)
Path(__file__).with_name('audit.json').write_text(json.dumps({'go_and_module_files':len(before),'content_and_file_list_unchanged':True,'go_git_status_unchanged':True,'readonly_plans_unchanged':list(plans),'relative_links_checked':len(links),'diff_check':0},indent=2)+'\n')
print('audit passed:',len(before),'Go/module files; readonly plans and links checked')
