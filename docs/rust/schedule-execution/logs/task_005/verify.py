"""Audit Go/read-only plans, frozen provenance, validation and local lock changes."""
import hashlib,json,os,re,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_005')
before=json.loads((out/'before.json').read_text());files={}
for root,dirs,names in os.walk('.'):
 dirs[:]=[d for d in dirs if d!='.git']
 for n in names:
  if n.endswith('.go') or n in ('go.mod','go.sum','go.work','go.work.sum'):
   p=str(Path(root)/n);files[p]=hashlib.sha256(Path(p).read_bytes()).hexdigest()
status=subprocess.check_output(['git','status','--short','--untracked-files=all','--','*.go','go.mod','go.sum','go.work','go.work.sum','**/go.mod','**/go.sum'],text=True)
result=dict(before_count=len(before['files']),after_count=len(files),changed=[p for p in sorted(set(files)|set(before['files'])) if files.get(p)!=before['files'].get(p)],before_status=before['status'],after_status=status,plans_checked=len(before['plans']),changed_plans=[p for p,h in before['plans'].items() if hashlib.sha256(Path(p).read_bytes()).hexdigest()!=h])
(out/'go-verification.json').write_text(json.dumps(result,indent=2)+'\n');assert not result['changed'] and status==before['status'] and not result['changed_plans']
fpath=Path('docs/rust/schedule-execution/fixtures/task_005.json');fixture=json.loads(fpath.read_text());assert hashlib.sha256((out/'oracle.json').read_bytes()).hexdigest()==fixture['raw_output_sha256']
raw=json.loads((out/'oracle.json').read_text());assert raw['exit_code']==0 and 'commands:' not in raw['stdout'] and raw['stdout'].count('"CASE ')==9
fault=json.loads((out/'fault.json').read_text());assert fault['exit_code']==0 and '~panic: interface {}(*runtime.TypeAssertionError)' in fault['stdout']
safe=json.loads((out/'safe-tests.json').read_text());assert safe['exit_code']==0 and safe['stdout'].count('--- PASS:')==6 and 'FAIL' not in safe['stdout']
for r in json.loads((out/'sources.json').read_text()):assert hashlib.sha256(Path(r['path']).read_bytes()).hexdigest()==r['sha256']
provenance=json.loads((out/'provenance.json').read_text());prior=json.loads(Path('docs/rust/schedule-execution/logs/task_003/compilation.json').read_text());assert provenance['binaries']['schedulers.test']==next(x['binary_sha256'] for x in prior if x['package']=='./pkg/schedule/schedulers')
mutation=json.loads((out/'mutation.json').read_text());assert len(mutation)==4 and all(r['exit_code']==101 and r['failed']>0 for r in mutation)
validation=json.loads((out/'validation.json').read_text());assert len(validation)==7 and all(r['exit_code']==0 for r in validation);assert validation[1]['passed']==3 and validation[2]['passed']==3 and validation[6]['passed']>0
old=subprocess.check_output(['git','show','9f430953c:Cargo.lock'],text=True);new=Path('Cargo.lock').read_text();assert [b for b in new.split('[[package]]') if 'name = "astersql-pd-schedule-filter"\n' not in b]==old.split('[[package]]')
metadata=json.loads((out/'validation-3.out').read_text());packages={p['name']:p for p in metadata['packages']};seen=set();pending=['astersql-pd-schedule-filter']
while pending:
 name=pending.pop()
 if name in seen or name not in packages:continue
 seen.add(name);pending.extend(d['name'] for d in packages[name]['dependencies'] if d['kind'] is None)
assert not ({'astersql-pd-server','astersql-pd-runtime','astersql-pd-proto'}&seen)
summary=dict(fixture_sha256=hashlib.sha256(fpath.read_bytes()).hexdigest(),go_files_unchanged=True,read_only_plans_unchanged=True,only_local_lockfile_package_added=True,production_dependency_direction_valid=True,go_tests=6,go_selection_cases=8,go_state_filter_context_only=1,go_resource_fault_cases=1,rust_focused_tests=3,rust_package_tests=3,rust_workspace_tests=validation[6]['passed'],rust_workspace_ignored=validation[6]['ignored'],mutation_checks=4)
(out/'evidence-verification.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary,indent=2))

for p in [Path('docs/rust/schedule-execution/task-005.md'),Path('pkg/schedule/filter/selection.rs.md')]:
 for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
  if '://' not in link:assert (p.parent/link.split('#')[0]).exists(),(p,link)
