"""Verify frozen Go sources and compare task-start readonly inventory."""
from pathlib import Path
import subprocess,json,hashlib
sha=lambda b:hashlib.sha256(b).hexdigest()
fixture=Path('docs/rust/schedule-execution/fixtures/task_018.json');f=json.loads(fixture.read_text())
for p,h in f['sources'].items():
 assert sha(Path(p).read_bytes())==h,p
 assert sha(subprocess.check_output(['git','show',f['source_commit']+':'+p]))==h,p
before=json.loads(Path('/tmp/pd-task018-before.json').read_text())
paths=subprocess.check_output(['git','ls-files','-co','--exclude-standard','-z']).decode().split('\0')
after={p:sha(Path(p).read_bytes()) for p in set(paths) if p and Path(p).is_file() and (p.endswith('.go') or Path(p).name in ['go.mod','go.sum','go.work','go.work.sum'] or p.endswith('/plan.md'))}
assert before==after,'readonly file inventory changed'
assert not subprocess.check_output(['git','diff','--name-only','--','*.go','go.mod','go.sum','go.work','go.work.sum','**/go.mod','**/go.sum']).strip()
subprocess.run(['git','diff','--check'],check=True)
log=Path('docs/rust/schedule-execution/logs/task_018')
results=json.loads((log/'validation.json').read_text());assert len(results)==7 and all(r['exit_code']==0 for r in results)
assert not Path('target/rust-slot-locks/slot-1.lock').exists()
report=dict(readonly_inventory_count=len(before),readonly_inventory_sha256=sha(json.dumps(before,sort_keys=True).encode()),go_and_module_files_unchanged=sum(p.endswith('.go') or Path(p).name.startswith('go.') for p in before),plans_unchanged=True,fixture_sha256=sha(fixture.read_bytes()),source_commit=f['source_commit'],evidence_kind=f['evidence_kind'],go_execution=False,focused_tests=results[1]['passed'],package_tests=results[2]['passed'],workspace_tests=results[6]['passed'],workspace_ignored=results[6]['ignored'],slot=1,cargo_target_dir=results[1]['cargo_target_dir'],slot_released=True,diff_check_exit_code=0,lock_changes='two existing workspace path dependencies; no remote version changes')
(log/'verification.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
