"""Check Go contents, source provenance and read-only plans against task start."""
from pathlib import Path
import hashlib,json,subprocess,re
out=Path('docs/rust/schedule-execution/logs/task_035')
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
before=json.loads((out/'before.json').read_text())
paths=subprocess.check_output(['rg','--files','--hidden','-g','!.git','-g','!target','-g','*.go','-g','go.mod','-g','go.sum','-g','go.work','-g','go.work.sum'],text=True).splitlines()
after={p:sha(p) for p in paths}
assert before['go']==after,'Go or module content changed'
assert all(sha(p)==h for p,h in before['plans'].items()),'read-only plan changed'
f=Path('docs/rust/schedule-execution/fixtures/task_035.json');fixture=json.loads(f.read_text())
for p,h in fixture['sources'].items():
    assert sha(p)==h
    assert hashlib.sha256(subprocess.check_output(['git','show',fixture['source_commit']+':'+p])).hexdigest()==h
status=subprocess.check_output(['git','status','--porcelain','--untracked-files=all','--','*.go','go.mod','go.sum','go.work','go.work.sum','**/go.mod','**/go.sum','**/go.work','**/go.work.sum'],text=True)
assert not status,status
subprocess.run(['git','diff','--check'],check=True)
(out/'verification.json').touch(exist_ok=True)
for p in [Path('docs/rust/schedule-execution/task-035.md'),Path('pkg/schedule/operator/controller_limit.rs.md')]:
    for target in re.findall(r'\]\(([^)]+)\)',p.read_text()):
        if '://' not in target:
            assert (p.parent/target.split('#')[0]).exists(),(str(p),target)
record=dict(go_files=len(after),go_hashes_unchanged=True,go_git_status=status,readonly_plans_unchanged=True,source_matches_commit=True,fixture_sha256=sha(f),diff_check_exit_code=0,markdown_links_valid=True)
(out/'verification.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record,indent=2))
commands={}
for name in ['fmt','focused','package','metadata','fmt-check','workspace-check','workspace-test']:
    result=json.loads((out/(name+'.json')).read_text())
    assert result['exit_code']==0,(name,result)
    assert result['CARGO_TARGET_DIR']==str(Path.cwd()/'target'/f"rust-slot-{result['slot']}")
    assert 1<=result['slot']<=10
    content=(out/(name+'.log')).read_text()
    rows=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',content)
    result['tests']={key:sum(int(row[i]) for row in rows) for i,key in enumerate(['passed','failed','ignored'])}
    commands[name]=result
assert commands['focused']['tests']==dict(passed=10,failed=0,ignored=0)
assert commands['package']['tests']==dict(passed=75,failed=0,ignored=0)
assert commands['workspace-test']['tests']==dict(passed=1438,failed=0,ignored=1)
for owner in Path('target/rust-slot-locks').glob('slot-*/owner.json'):
    assert json.loads(owner.read_text()).get('session')!='task-035','own Cargo slot still held'
record['commands']=commands
record['own_cargo_slots_released']=True
record['rust_sha256']={str(p):sha(p) for p in map(Path,['pkg/schedule/operator/controller_limit.rs','pkg/schedule/operator/task_035_test.rs','pkg/schedule/operator/controller_admission.rs','pkg/schedule/operator/lib.rs'])}
(out/'verification.json').write_text(json.dumps(record,indent=2)+'\n')
print('Final validation: focused10 / package75 / workspace1438 passed; own slots released')

normalization=json.loads((out/'log-normalization.json').read_text())
for name, entry in normalization.items():
    reconstructed=(out/name).read_bytes()+b'\n'*entry['removed_eof_newlines']
    assert hashlib.sha256(reconstructed).hexdigest()==entry['raw_sha256']
record['log_eof_normalization_reconstructs_raw']=True
(out/'verification.json').write_text(json.dumps(record,indent=2)+'\n')
