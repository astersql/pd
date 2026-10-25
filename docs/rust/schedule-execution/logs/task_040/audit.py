"""Compare every Go/module file with the baseline captured before task edits."""
from pathlib import Path
import subprocess,hashlib,json
out=Path(__file__).resolve().parent
sha=lambda b:hashlib.sha256(b).hexdigest()
before=json.loads((out/'go-before.json').read_text())
paths=subprocess.check_output(['git','ls-files','-co','--exclude-standard','-z']).decode().split('\0')
after={p:sha(Path(p).read_bytes()) for p in paths if p and Path(p).is_file() and (p.endswith('.go') or Path(p).name in ['go.mod','go.sum','go.work','go.work.sum'])}
changed=[p for p in before.keys()|after.keys() if before.get(p)!=after.get(p)]
plans=['.plans/2026-10-25-调度执行内核/plan.md','.plans/2026-10-25-调度执行内核/05-检查器/plan.md']
plan_checks={p:Path(p).read_bytes()==subprocess.check_output(['git','show',f'HEAD:{p}']) for p in plans}
status=subprocess.check_output(['git','status','--porcelain','--untracked-files=all']).decode()
go_status=[s for s in status.splitlines() if s[3:] in before or s[3:] in after]
go_before=json.loads((out/'go-status-before.json').read_text())
fixture=Path('docs/rust/schedule-execution/fixtures/task_040.json')
result=dict(file_count=len(before),before_manifest_sha256=sha(json.dumps(before,sort_keys=True).encode()),after_manifest_sha256=sha(json.dumps(after,sort_keys=True).encode()),changed_go_module_files=changed,go_status_before=go_before,go_status_after=go_status,readonly_plans_match_HEAD=plan_checks,fixture_sha256=sha(fixture.read_bytes()),diff_check_exit=subprocess.run(['git','diff','--check']).returncode)
(out/'audit.json').write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n')
assert not changed and go_before==go_status and all(plan_checks.values()) and result['diff_check_exit']==0
print(json.dumps(result,indent=2,ensure_ascii=False))
