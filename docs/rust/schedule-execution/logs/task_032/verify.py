"""Verify protected source snapshots, task links and repository differences."""
import hashlib,json,re,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_032')
before=json.loads((out/'protected-before.json').read_text())
protected=lambda p:p.suffix=='.go' or p.name in ['go.mod','go.sum','go.work','go.work.sum','plan.md']
now={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in Path('.').rglob('*') if '.git' not in p.parts and 'target' not in p.parts and p.is_file() and protected(p)}
changes={p:[before.get(p),now.get(p)] for p in before.keys()|now.keys() if before.get(p)!=now.get(p)}
status=subprocess.check_output(['git','status','--porcelain','-z','-uall']).decode().split('\0')
go_status=[s for s in status if s and protected(Path(s[3:]))]
links=[]
for path in [Path('pkg/schedule/operator/create_range.rs.md'),Path('docs/rust/schedule-execution/task-032.md')]:
 for link in re.findall(r'\]\(([^)]+)\)',path.read_text()):
  if not link.startswith(('http:','https:','#')) and not (path.parent/link.split('#')[0]).exists():links.append([str(path),link])
diff=subprocess.run(['git','diff','--check'],capture_output=True,text=True)
result=dict(protected_files=len(before),go_module_files=sum(Path(p).name!='plan.md' for p in before),protected_hash_changes=changes,protected_git_status=go_status,broken_links=links,diff_check_exit_code=diff.returncode,diff_check_output=diff.stdout+diff.stderr,fixture_sha256=hashlib.sha256(Path('docs/rust/schedule-execution/fixtures/task_032.json').read_bytes()).hexdigest())
(out/'verification.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
assert not changes and not go_status and not links and diff.returncode==0
