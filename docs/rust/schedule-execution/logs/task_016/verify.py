"""Audit read-only boundaries and captured task016 evidence (no Cargo calls)."""
import hashlib,json,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_016')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z']).decode().split('\0')
paths=sorted(set(p for p in paths if p and (p.endswith('.go') or Path(p).name in ['go.mod','go.sum','go.work','go.work.sum'])))
h=hashlib.sha256()
for p in paths:
 h.update(p.encode()+b'\0'+(hashlib.sha256(Path(p).read_bytes()).digest() if Path(p).exists() else b'missing'))
after=dict(files=len(paths),sha256=h.hexdigest(),status=subprocess.check_output(['git','status','--porcelain','--','*.go','go.mod','go.sum','go.work','go.work.sum']).decode())
assert after==json.loads((out/'go-before.json').read_text()),'Go or module file changed'
for p,expected in json.loads((out/'plans-before.json').read_text()).items():
 assert sha(Path(p))==expected,p
fixture=Path('docs/rust/schedule-execution/fixtures/task_016.json');data=json.loads(fixture.read_text())
assert data['evidence_kind']=='source_assertion' and data['go_execution'] is False
for p,expected in data['sources'].items():
 assert sha(Path(p))==expected
 assert hashlib.sha256(subprocess.check_output(['git','show',data['source_commit']+':'+p])).hexdigest()==expected
for r in json.loads((out/'red.json').read_text()):
 assert r['exit_code']==101 and 'FAILED' in (out/r['log']).read_text()
validation=json.loads((out/'validation.json').read_text())
assert len(validation)==7 and all(r['exit_code']==0 for r in validation)
assert validation[1]['passed']==3 and validation[2]['passed']==7
assert validation[-1]['passed']>0 and validation[-1]['failed']==0
assert not subprocess.run(['git','diff','--exit-code','--','Cargo.lock'],capture_output=True).returncode
assert not subprocess.run(['git','diff','--check'],capture_output=True).returncode
assert not Path('target/rust-slot-locks/slot-1.lock').exists()
provenance=json.loads((out/'fixture-reuse-provenance.json').read_text())
assert sha(Path(provenance['binary']))==provenance['binary_sha256']
assert sha(Path(provenance['source']))==provenance['source_sha256']
assert hashlib.sha256(subprocess.check_output(['git','show',provenance['binary_vcs_revision']+':'+provenance['source']])).hexdigest()==provenance['source_sha256']
result=dict(go_after=after,plans_unchanged=True,source_commit_verified=True,fixture_sha256=sha(fixture),fixture_cases=len(data['cases']),mutations_rejected=2,focused_passed=3,package_passed=7,workspace_passed=validation[-1]['passed'],workspace_ignored=validation[-1]['ignored'],cargo_lock_unchanged=True,slot=1,cargo_target_dir=validation[-1]['cargo_target_dir'],slot_released=True,etcd_reuse_requests=len((out/'go-reuse.jsonl').read_text().splitlines()))
(out/'verification.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
