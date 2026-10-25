"""Compare protected Go/module and readonly-plan content against entry state."""
from pathlib import Path
import subprocess,hashlib,json
out=Path(__file__).resolve().parent
root=out.parents[4]
before=json.loads((out/'before.json').read_text())
plans=['.plans/2026-10-25-调度执行内核/plan.md','.plans/2026-10-25-调度执行内核/05-检查器/plan.md']
files=[s for s in subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=root).decode().split('\0') if s and (s.endswith('.go') or Path(s).name in ['go.mod','go.sum','go.work','go.work.sum'] or s in plans)]
after={s:hashlib.sha256((root/s).read_bytes()).hexdigest() for s in files if (root/s).exists()}
status=subprocess.check_output(['git','status','--porcelain','--','*.go','go.mod','go.sum','go.work','go.work.sum'],cwd=root).decode()
assert before['hashes']==after and before['status']==status
r=subprocess.run(['git','diff','--check'],cwd=root,capture_output=True,text=True)
assert r.returncode==0,r.stdout
result=dict(protected_files=len(after),hashes_unchanged=True,status_unchanged=True,new_go_files=[],diff_check_exit_code=r.returncode,fixture_sha256=hashlib.sha256((root/'docs/rust/schedule-execution/fixtures/task_046.json').read_bytes()).hexdigest())
(out/'audit.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result))
