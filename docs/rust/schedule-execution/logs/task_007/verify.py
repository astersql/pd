"""Audit immutable Go/plan inputs, frozen oracle results and final Rust evidence."""
import hashlib,json,os,re,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_007');before=json.loads((out/'before.json').read_text());files={}
for root,ds,ns in os.walk('.'):
 ds[:]=[d for d in ds if d!='.git']
 for n in ns:
  if n.endswith('.go') or n in ('go.mod','go.sum','go.work','go.work.sum'):
   p=str(Path(root)/n);files[p]=hashlib.sha256(Path(p).read_bytes()).hexdigest()
changed=[p for p in sorted(set(files)|set(before['files'])) if files.get(p)!=before['files'].get(p)]
status=subprocess.check_output(['git','status','--short','--untracked-files=all','--','*.go','**/go.mod','**/go.sum','**/go.work','**/go.work.sum','go.mod','go.sum','go.work','go.work.sum'],text=True)
changed_plans=[p for p,h in before['plans'].items() if hashlib.sha256(Path(p).read_bytes()).hexdigest()!=h]
go=dict(before_count=len(before['files']),after_count=len(files),changed=changed,ending_go_status=status,changed_plans=changed_plans)
(out/'go-verification.json').write_text(json.dumps(go,indent=2)+'\n');assert not changed and not status and not changed_plans
assert not subprocess.check_output(['git','diff','--name-only','836fdc7cf3ef34f7eabc9d051418995bb97efabe','--','*.go','go.mod','go.sum'],text=True)
fp=Path('docs/rust/schedule-execution/fixtures/task_007.json');f=json.loads(fp.read_text());assert len(f['cases'])==243
for name,key,start in [('oracle.json','raw_sha256',0),('reject.json','reject_raw_sha256',234)]:
 assert hashlib.sha256((out/name).read_bytes()).hexdigest()==f[key]
 raw=json.loads((out/name).read_text());assert raw['exit_code']==0 and not raw['stderr'] and not re.search(r'commands:\d+:',raw['stdout'])
 chunks=re.split(r'"CASE ([^"]+)"',raw['stdout']);expected_cases=f['cases'][start:234 if not start else 243];assert len(chunks)==2*len(expected_cases)+1
 for case,cid,chunk in zip(expected_cases,chunks[1::2],chunks[2::2]):
  assert cid==case['id'];statuses=re.findall(r'StatusCode: (?:\w+ \()?(-?\d+)\)?, DetailedReason: "([^"]*)"',chunk);reasons=re.findall(r'^\w+ \((\d+)\)$',chunk,re.M)
  assert case['expected']==[dict(status=int(s),detail=d,reason=int(r)) for (s,d),r in zip(statuses,reasons)]
for row in json.loads((out/'sources.json').read_text()):assert hashlib.sha256(Path(row['path']).read_bytes()).hexdigest()==row['sha256']
prov=json.loads((out/'provenance.json').read_text());prior=json.loads(Path('docs/rust/schedule-execution/logs/task_006/provenance.json').read_text())
for n,h in prov['binaries'].items():assert h==prior['binaries'][n]==hashlib.sha256(Path('/tmp/pd-sched005',n).read_bytes()).hexdigest()
for name,n in [('safe-tests.json',3),('checker-safe-test.json',2)]:
 raw=json.loads((out/name).read_text());assert raw['exit_code']==0 and raw['stdout'].count('--- PASS:')==n
mutation=json.loads((out/'mutation.json').read_text());assert len(mutation)==3 and all(r['exit_code']==101 and r['failed']>0 for r in mutation)
val=json.loads((out/'validation.json').read_text());assert len(val)==7 and all(r['exit_code']==0 for r in val);assert val[1]['passed']==5 and val[2]['passed']==11 and val[6]['passed']>0
metadata=json.loads((out/'validation-3.out').read_text());packages={p['name']:p for p in metadata['packages']};seen=set();pending=['astersql-pd-schedule-filter']
while pending:
 n=pending.pop()
 if n in seen or n not in packages:continue
 seen.add(n);pending.extend(d['name'] for d in packages[n]['dependencies'] if d['kind'] is None)
assert not ({'astersql-pd-server','astersql-pd-runtime','astersql-pd-proto'}&seen)
# Only local dependency edges may change in Cargo.lock, never registry versions/checksums.
import tomllib
old=tomllib.loads(subprocess.check_output(['git','show','HEAD:Cargo.lock'],text=True));new=tomllib.loads(Path('Cargo.lock').read_text());assert [p for p in old['package'] if 'source' in p]==[p for p in new['package'] if 'source' in p]
subprocess.run(['git','diff','--check'],check=True)
summary=dict(fixture_sha256=hashlib.sha256(fp.read_bytes()).hexdigest(),go_files_unchanged=True,readonly_plans_checked=len(before['plans']),oracle_cases=243,oracle_source_target_results=486,complete_go_behavior_tests=4,mutation_failures=3,rust_focused_tests=5,rust_package_tests=11,rust_workspace_tests=val[6]['passed'],rust_workspace_ignored=val[6]['ignored'],production_dependencies_valid=True,registry_dependencies_unchanged=True,cargo_target_dir=val[6]['cargo_target_dir'])
(out/'evidence-verification.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary,indent=2))

for p in [Path('docs/rust/schedule-execution/task-007.md'),Path('pkg/schedule/filter/store_state.rs.md')]:
 for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
  if '://' not in link:assert (p.parent/link.split('#')[0]).exists(),(p,link)
