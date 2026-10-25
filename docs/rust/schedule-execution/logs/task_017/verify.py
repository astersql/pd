"""Verify the read-only boundary and saved task017 results without invoking Cargo."""
import hashlib,json,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_017')
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
paths=sorted(set(p for p in subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z']).decode().split('\0') if p and (p.endswith('.go') or Path(p).name in ['go.mod','go.sum','go.work','go.work.sum'])))
after={p:sha(p) if Path(p).exists() else None for p in paths}
assert after==json.loads((out/'go-before.json').read_text())
assert subprocess.check_output(['git','status','--porcelain','--','*.go','go.mod','go.sum','go.work','go.work.sum'])==json.loads((out/'go-status-before.json').read_text()).encode()
for p,h in json.loads((out/'readonly-before.json').read_text()).items():
 if p!='Cargo.lock':assert sha(p)==h,p
lockdiff=subprocess.check_output(['git','diff','--','Cargo.lock'],text=True)
assert [s for s in lockdiff.splitlines() if s.startswith('+') and not s.startswith('+++')]==['+ "astersql-pd-core",','+ "astersql-pd-core-constant",','+ "astersql-pd-storelimit",']
fixture=Path('docs/rust/schedule-execution/fixtures/task_017.json');data=json.loads(fixture.read_text())
assert not data['go_execution'] and data['evidence_kind']=='source_assertion'
for p,h in data['sources'].items():
 assert sha(p)==h
 assert hashlib.sha256(subprocess.check_output(['git','show',data['source_commit']+':'+p])).hexdigest()==h
red=json.loads((out/'red.json').read_text());assert red['exit_code']==101
assert 'task_017_test::task_017_behavior_cases ... FAILED' in (out/red['log']).read_text()
rows=json.loads((out/'validation.json').read_text());assert len(rows)==7 and all(r['exit_code']==0 for r in rows)
assert rows[1]['passed']==5 and rows[2]['passed']==12 and rows[-1]['passed']>0 and rows[-1]['failed']==0
assert not Path('target/rust-slot-locks/slot-1.lock').exists()
provenance=json.loads((out/'fixture-reuse-provenance.json').read_text())
assert sha(provenance['binary'])==provenance['binary_sha256']
assert sha(provenance['source'])==provenance['source_sha256']
assert hashlib.sha256(subprocess.check_output(['git','show',provenance['binary_vcs_revision']+':'+provenance['source']])).hexdigest()==provenance['source_sha256']
assert subprocess.run(['git','diff','--check']).returncode==0
result=dict(go_files_unchanged=len(after),plans_unchanged=True,lock_changes='three local dependencies only',fixture_sha256=sha(fixture),source_commit_verified=data['source_commit'],evidence_kind=data['evidence_kind'],focused_tests=rows[1]['passed'],package_tests=rows[2]['passed'],workspace_tests=rows[-1]['passed'],workspace_ignored=rows[-1]['ignored'],slot=1,cargo_target_dir=rows[-1]['cargo_target_dir'],slot_released=True)
(out/'verification.json').write_text(json.dumps(result,indent=2)+'\n');print(result)
