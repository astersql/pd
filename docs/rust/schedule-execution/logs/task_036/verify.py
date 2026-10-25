"""Verify read-only inputs, source provenance and task-specific validation evidence."""
from pathlib import Path
import hashlib,json,subprocess,re
out=Path('docs/rust/schedule-execution/logs/task_036')
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
before=json.loads((out/'baseline.json').read_text())
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z']).decode().split('\0')
restricted=lambda p:p.endswith('.go') or Path(p).name in ('go.mod','go.sum','go.work','go.work.sum')
after={p:sha(p) for p in paths if p and restricted(p) and Path(p).is_file()}
assert before['go']==after,'Go or module content changed'
assert all(sha(p)==h for p,h in before['plans'].items()),'read-only plan changed'
f=Path('docs/rust/schedule-execution/fixtures/task_036.json');fixture=json.loads(f.read_text())
assert len(fixture['cases'])==7
for p,h in fixture['source_sha256'].items():
    assert sha(p)==h
    assert hashlib.sha256(subprocess.check_output(['git','show',fixture['source_commit']+':'+p])).hexdigest()==h
status=subprocess.check_output(['git','status','--porcelain','--untracked-files=all','--','*.go','go.mod','go.sum','go.work','go.work.sum','**/go.mod','**/go.sum','**/go.work','**/go.work.sum'],text=True)
assert not status,status
subprocess.run(['git','diff','--check'],check=True)
(out/'verification.json').touch(exist_ok=True)
for p in [Path('docs/rust/schedule-execution/task-036.md'),Path('pkg/schedule/operator/controller_dispatch.rs.md')]:
    for target in re.findall(r'\]\(([^)]+)\)',p.read_text()):
        if '://' not in target:assert (p.parent/target.split('#')[0]).exists(),(str(p),target)
commands={}
for name in ['fmt','focused','package','metadata','fmt-check','workspace-check','workspace-test','tso-two-threads']:
    result=json.loads((out/(name+'.json')).read_text())
    assert result['exit_code']==0,(name,result)
    assert result['CARGO_TARGET_DIR']==str(Path.cwd()/'target'/f"rust-slot-{result['slot']}")
    assert 1<=result['slot']<=10
    content=(out/(name+'.log')).read_text()
    rows=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',content)
    result['tests']={key:sum(int(row[i]) for row in rows) for i,key in enumerate(['passed','failed','ignored'])}
    commands[name]=result
assert commands['tso-two-threads']['tests']==dict(passed=1,failed=0,ignored=0)
assert commands['focused']['tests']==dict(passed=10,failed=0,ignored=0)
assert commands['package']['tests']==dict(passed=85,failed=0,ignored=0)
assert commands['workspace-test']['tests']==dict(passed=1448,failed=0,ignored=1)
assert commands['workspace-test']['test_env']=={'RUST_TEST_THREADS':'2'}
assert json.loads((out/'mutation.json').read_text())['exit_code']==101
assert '1 failed' in (out/'mutation.log').read_text()
for owner in Path('target/rust-slot-locks').glob('slot-*/owner.json'):
    assert json.loads(owner.read_text()).get('session')!='task-036','own Cargo slot still held'
normalization=json.loads((out/'log-normalization.json').read_text())
for name,entry in normalization.items():
    reconstructed=(out/name).read_bytes()+b'\n'*entry['removed_eof_newlines']
    assert hashlib.sha256(reconstructed).hexdigest()==entry['raw_sha256']
record=dict(go_files=len(after),go_hashes_unchanged=True,go_git_status=status,readonly_plans_unchanged=True,source_matches_commit=True,fixture_sha256=sha(f),diff_check_exit_code=0,markdown_links_valid=True,commands=commands,own_cargo_slots_released=True,log_eof_normalization_reconstructs_raw=True)
record['parallel_regression_failure']=json.loads((out/'workspace-parallel-failure.json').read_text())
record['single_thread_regression_failure']=json.loads((out/'workspace-single-thread-failure.json').read_text())
record['rust_sha256']={str(p):sha(p) for p in map(Path,['pkg/schedule/operator/controller_dispatch.rs','pkg/schedule/operator/task_036_test.rs','pkg/schedule/operator/controller_admission.rs','pkg/schedule/operator/lib.rs'])}
(out/'verification.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record,indent=2))
