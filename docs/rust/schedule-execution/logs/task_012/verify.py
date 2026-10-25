"""Audit frozen raw outputs, readonly boundary, dependencies, and validation."""
import json,hashlib,subprocess,re
from pathlib import Path
out=Path(__file__).parent
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
before=json.loads((out/'before.json').read_text())
paths=subprocess.check_output(['git','ls-files','-co','--exclude-standard','-z']).decode().split('\0')
after={p:sha(p) for p in set(paths) if Path(p).is_file() and (p.endswith('.go') or Path(p).name in ['go.mod','go.sum','go.work','go.work.sum','plan.md'])}
assert after==before['hashes']
status=subprocess.check_output(['git','status','--porcelain','-z']).decode().split('\0')
assert not any(r and (r.endswith('.go') or Path(r[3:]).name in ['go.mod','go.sum','go.work','go.work.sum']) for r in status)
fixture=json.load(open('docs/rust/schedule-execution/fixtures/task_012.json'))
oracle=json.loads((out/'oracle.json').read_text());assert oracle['exit_code']==0
marshals=[r for r in oracle['exchanges'] if '.Marshal(' in r['command']]
observed=[r for r in oracle['exchanges'] if r['command'].startswith('print ') and '.msgCh.buf[' in r['command']]
actual=[json.loads(bytes(int(n) for n in re.search(r'\[([0-9, ]+)\]',r['output'])[1].split(','))) for r in marshals]
assert actual==[c['output'] for c in fixture['cases'] if c['output'] is not None]
assert len(actual)==32 and len(observed)==32 and len(fixture['cases'])==34
assert sum('schedulingpb.RegionHeartbeatResponse' in r['output'] for r in observed)==16
assert sum('pdpb.RegionHeartbeatResponse' in r['output'] for r in observed)==16
no_leader=[r for r in oracle['exchanges'] if '.MsgLength()' in r['command']]
assert len(no_leader)==2 and all('~r0: 0' in r['output'] for r in no_leader)
provenance=json.loads((out/'provenance.json').read_text())
for group in ['binaries','sources']:
 for p,digest in provenance[group].items():assert sha(p)==digest,p
assert provenance['fixture_sha256']==sha('docs/rust/schedule-execution/fixtures/task_012.json')
subprocess.run(['git','merge-base','--is-ancestor',provenance['dependency_commit'],'HEAD'],check=True)
assert '1353通过' in Path('docs/rust/schedule-execution/task-011.md').read_text()
mutations=json.loads((out/'mutation.json').read_text());assert len(mutations)==12
assert all(r['exit_code']==101 and r['failed']>0 for r in mutations)
for r in mutations:
 s=(out/(r['mutation']+'.out')).read_text();assert 'assertion `left == right` failed' in s and 'error: could not compile' not in s
results=json.loads((out/'validation.json').read_text());assert len(results)==7 and all(r['exit_code']==0 for r in results)
assert results[1]['passed']==results[2]['passed']==2
assert results[6]['passed']>=1355 and results[6]['failed']==0
# No unrequested registry dependency changes from offline lock regeneration.
old=subprocess.check_output(['git','show','HEAD:Cargo.lock']).decode()
new=Path('Cargo.lock').read_text()
registry=lambda s:[x for x in s.split('[[package]]') if 'source = ' in x]
assert registry(old)==registry(new)
assert 'proto' not in Path('pkg/schedule/hbstream/Cargo.toml').read_text()
for lock in Path('target/rust-slot-locks').glob('slot-*.lock'):
 owner=lock/'owner'
 assert not (owner.exists() and 'schedule-heartbeat-command-012' in owner.read_text())
subprocess.run(['git','diff','--check'],check=True)
report=dict(go_and_plan_hashes_unchanged=len(after),go_module_differences=0,go_runtime_cases=34,go_classic_outputs=16,go_scheduling_outputs=16,no_leader_suppressed=2,rust_tests=2,mutation_assertion_failures=12,workspace_passed=results[6]['passed'],workspace_ignored=results[6]['ignored'],remote_dependencies_unchanged=True,task_locks_released=True)
(out/'verification.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
