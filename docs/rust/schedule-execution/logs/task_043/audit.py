"""Check readonly Go/module and plan content against the starting snapshot."""
from pathlib import Path
import subprocess,json,hashlib
out=Path(__file__).resolve().parent;root=out.parents[4];before=json.loads((out/'before.json').read_text())
paths=subprocess.check_output(['git','ls-files','-co','--exclude-standard','-z'],cwd=root).decode().split('\0')
def relevant(p):return p.endswith('.go') or Path(p).name in ['go.mod','go.sum','go.work','go.work.sum']
after={p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in set(paths) if p and relevant(p) and (root/p).is_file()}
diff=[p for p in sorted(before['go'].keys()|after.keys()) if before['go'].get(p)!=after.get(p)]
plans={p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in before['plans']}
fixture=root/'docs/rust/schedule-execution/fixtures/task_043.json'
record=dict(go_files=len(after),go_differences=diff,go_aggregate_sha256=hashlib.sha256(json.dumps(after,sort_keys=True).encode()).hexdigest(),readonly_plans_unchanged=plans==before['plans'],fixture_sha256=hashlib.sha256(fixture.read_bytes()).hexdigest(),go_git_status=subprocess.check_output(['git','status','--porcelain','--','*.go','go.mod','go.sum','go.work','go.work.sum'],cwd=root).decode(),diff_check_exit=subprocess.run(['git','diff','--check'],cwd=root).returncode)
(out/'audit.json').write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record))
assert not diff and record['readonly_plans_unchanged'] and not record['go_git_status'] and not record['diff_check_exit']
