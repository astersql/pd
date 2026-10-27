"""Compare protected source contents/status and evidence links without invoking Go/Cargo."""
from pathlib import Path
import hashlib,json,re,subprocess
folder=Path('docs/rust/schedule-execution/logs/task_147')
baseline=json.loads((folder/'baseline.json').read_text())
files=subprocess.check_output(['rg','--files','--hidden','-g','!.git','-g','!target']).decode().splitlines()
go={p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in files if p.endswith('.go') or Path(p).name in ['go.mod','go.sum','go.work','go.work.sum']}
status=subprocess.check_output(['git','status','--porcelain','--','*.go','go.mod','go.sum','go.work','go.work.sum']).decode()
plans={p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in baseline['plans']}
assert go==baseline['go_files'];assert status==baseline['go_status'];assert plans==baseline['plans']
links=[]
for p in [Path('docs/rust/schedule-execution/task-147.md'),Path('pkg/server/schedule_admin_random_merge.rs.md')]:
 for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
  target=(p.parent/link).resolve()
  assert target==(folder/'audit.json').resolve() or target.exists(),(p,link)
  links.append(str(p.parent/link))
p=subprocess.run(['git','diff','--check'],capture_output=True,text=True)
assert p.returncode==0,p.stdout+p.stderr
(folder/'audit.json').write_text(json.dumps(dict(go_file_count=len(go),go_contents_and_status_unchanged=True,plans_unchanged=True,links=links,diff_check_exit_code=p.returncode),indent=2)+'\n')
