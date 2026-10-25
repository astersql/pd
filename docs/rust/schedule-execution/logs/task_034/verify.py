"""Check Go contents, source provenance and read-only plans against task start."""
from pathlib import Path
import hashlib,json,subprocess,re
out=Path('docs/rust/schedule-execution/logs/task_034')
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
before=json.loads((out/'before.json').read_text())
paths=subprocess.check_output(['rg','--files','--hidden','-g','!.git','-g','!target','-g','*.go','-g','go.mod','-g','go.sum','-g','go.work','-g','go.work.sum'],text=True).splitlines()
after={p:sha(p) for p in paths}
assert before['go']==after,'Go or module content changed'
assert all(sha(p)==h for p,h in before['plans'].items()),'read-only plan changed'
f=Path('docs/rust/schedule-execution/fixtures/task_034.json');fixture=json.loads(f.read_text())
for p,h in fixture['sources'].items():
    assert sha(p)==h
    assert hashlib.sha256(subprocess.check_output(['git','show',fixture['source_commit']+':'+p])).hexdigest()==h
status=subprocess.check_output(['git','status','--porcelain','--untracked-files=all','--','*.go','go.mod','go.sum','go.work','go.work.sum','**/go.mod','**/go.sum','**/go.work','**/go.work.sum'],text=True)
assert not status,status
subprocess.run(['git','diff','--check'],check=True)
(out/'verification.json').touch(exist_ok=True)
for p in [Path('docs/rust/schedule-execution/task-034.md'),Path('pkg/schedule/operator/controller_admission.rs.md')]:
    for target in re.findall(r'\]\(([^)]+)\)',p.read_text()):
        if '://' not in target:
            assert (p.parent/target.split('#')[0]).exists(),(str(p),target)
record=dict(go_files=len(after),go_hashes_unchanged=True,go_git_status=status,readonly_plans_unchanged=True,source_matches_commit=True,fixture_sha256=sha(f),diff_check_exit_code=0,markdown_links_valid=True)
(out/'verification.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record,indent=2))
