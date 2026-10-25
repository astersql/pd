"""Confirm read-only sources, fixtures, and final validation counts."""
from pathlib import Path
import hashlib,json,subprocess,re
out=Path(__file__).resolve().parent
root=out.parents[4]
sha=lambda b:hashlib.sha256(b).hexdigest()
before=json.loads((out/'go-before.json').read_text())
files=subprocess.check_output(['git','ls-files','-co','--exclude-standard','-z'],cwd=root).decode().split('\0')
files=sorted(set(f for f in files if f and (f.endswith('.go') or Path(f).name in ['go.mod','go.sum','go.work','go.work.sum'])))
after={f:sha((root/f).read_bytes()) for f in files if (root/f).is_file()}
status=subprocess.check_output(['git','status','--porcelain','--','*.go','go.mod','go.sum','go.work','go.work.sum'],cwd=root).decode()
assert before['hashes']==after,'Go/module contents changed'
assert before['status']==status,'Go/module status changed'
fixture=root/'docs/rust/schedule-execution/fixtures/task_039.json'
f=json.loads(fixture.read_text())
sources=[]
for path,expected in f['sources'].items():
 current=sha((root/path).read_bytes()); base=sha(subprocess.check_output(['git','show',f['source_commit']+':'+path],cwd=root))
 assert current==expected
 sources.append(dict(path=path,current_sha256=current,source_commit_sha256=base,matches_baseline=current==base))
plans=['.plans/2026-10-25-调度执行内核/plan.md','.plans/2026-10-25-调度执行内核/05-检查器/plan.md']
for plan in plans:
 assert (root/plan).read_bytes()==subprocess.check_output(['git','show','HEAD:'+plan],cwd=root)
audit=dict(checked_go_module_files=len(after),contents_unchanged=True,status_unchanged=True,status=status,fixture_sha256=sha(fixture.read_bytes()),sources=sources,readonly_plans_unchanged=plans,go_execution=False,evidence_kind='source_assertion')
(out/'audit.json').write_text(json.dumps(audit,indent=2)+'\n')
commands=json.loads((out/'commands.json').read_text())
assert len(commands)==7 and all(c['exit_code']==0 for c in commands)
assert json.loads((out/'validation.json').read_text())['exit_code']==0
log=(out/'validation.log').read_text()
assert 'test result: FAILED' not in log
assert 'task_039 fixture: 24 cases passed, 0 failed' in log
crate_log=log.split('COMMAND ["cargo", "test", "-p", "astersql-pd-schedule-checker", "--locked"]\n')[1].split('COMMAND ')[0]
crate_counts=[tuple(map(int,m)) for m in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',crate_log)]
assert sum(c[0] for c in crate_counts)==2 and all(c[1:]==(0,0) for c in crate_counts)
workspace=log.split('COMMAND ["cargo", "test", "--workspace", "--locked"]\n')[1]
counts=[tuple(map(int,m)) for m in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',workspace)]
assert counts
summary=dict(workspace=dict(passed=sum(c[0] for c in counts),failed=sum(c[1] for c in counts),ignored=sum(c[2] for c in counts)),crate=dict(passed=2,failed=0,ignored=0),fixture=dict(cases=len(f['cases']),passed=len(f['cases']),failed=0))
assert summary['workspace']['failed']==0
(out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps(summary));print('Go/module unchanged:',len(after))
