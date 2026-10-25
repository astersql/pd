"""Verify readonly provenance, nonzero tests, restored production and slot cleanup."""
from pathlib import Path
import hashlib,json,subprocess
sha=lambda b:hashlib.sha256(b).hexdigest()
log=Path('docs/rust/schedule-execution/logs/task_026')
fixture=Path('docs/rust/schedule-execution/fixtures/task_026.json');f=json.loads(fixture.read_text())
for p,h in f['source_sha256'].items():
 assert sha(Path(p).read_bytes())==h,p
 assert sha(subprocess.check_output(['git','show',f['source_commit']+':'+p]))==h,p
before=json.loads((log/'before.json').read_text())
paths=subprocess.check_output(['git','ls-files','-co','--exclude-standard','-z']).decode().split('\0')
after={p:sha(Path(p).read_bytes()) for p in set(paths) if p and Path(p).is_file() and (p.endswith('.go') or Path(p).name in ['go.mod','go.sum','go.work','go.work.sum'] or p.endswith('/plan.md'))}
assert before==after,'readonly inventory changed'
subprocess.run(['git','diff','--check'],check=True)
results=json.loads((log/'validation.json').read_text());assert len(results)==7 and all(r['exit_code']==0 for r in results)
assert results[1]['passed']==2 and results[2]['passed']==42 and results[6]['passed']>0
red=json.loads((log/'red.json').read_text());assert len(red['results'])==4 and all(r['exit_code']==101 for r in red['results'])
assert red['restored_sha256']==sha(Path('pkg/schedule/operator/builder_target.rs').read_bytes())
slots=sorted({int(Path(r['cargo_target_dir']).name.removeprefix('rust-slot-')) for r in results+red['results']})
for slot in slots:
 owner=Path(f'target/rust-slot-locks/slot-{slot}.lock/owner')
 if owner.exists():assert json.loads(owner.read_text()).get('task')!='026','own lock still held'
report=dict(readonly_inventory_count=len(before),go_and_module_files_unchanged=sum(p.endswith('.go') or Path(p).name.startswith('go.') for p in before),plans_unchanged=True,fixture_sha256=sha(fixture.read_bytes()),source_commit=f['source_commit'],evidence_kind=f['evidence_kind'],go_execution=False,fixture_cases=len(f['cases']),focused_tests=results[1]['passed'],package_tests=results[2]['passed'],workspace_tests=results[6]['passed'],workspace_ignored=results[6]['ignored'],slots=slots,cargo_target_dir=results[1]['cargo_target_dir'],slot_released=True,diff_check_exit_code=0)
(log/'verification.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
