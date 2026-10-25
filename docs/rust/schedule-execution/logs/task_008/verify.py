"""Audit frozen Go outputs, immutable source inputs and final verification."""
import hashlib,json,os,re,subprocess,tomllib
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_008');before=json.loads((out/'before.json').read_text());files={}
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
for root,ds,ns in os.walk('.'):
 ds[:]=[d for d in ds if d!='.git']
 for n in ns:
  if n.endswith('.go') or n in ('go.mod','go.sum','go.work','go.work.sum'):
   p=str(Path(root)/n);files[p]=sha(p)
changed=[p for p in sorted(set(files)|set(before['files'])) if files.get(p)!=before['files'].get(p)]
status=subprocess.check_output(['git','status','--short','--untracked-files=all','--','*.go','**/go.mod','**/go.sum','**/go.work','**/go.work.sum','go.mod','go.sum','go.work','go.work.sum'],text=True)
plans=[p for p,h in before['plans'].items() if sha(p)!=h]
(out/'go-verification.json').write_text(json.dumps(dict(before_count=len(before['files']),after_count=len(files),changed=changed,ending_go_status=status,changed_plans=plans),indent=2)+'\n');assert not changed and not status and not plans
assert not subprocess.check_output(['git','diff','--name-only','836fdc7cf3ef34f7eabc9d051418995bb97efabe','--','*.go','go.mod','go.sum'],text=True)
f=json.loads(Path('docs/rust/schedule-execution/fixtures/task_008.json').read_text());assert len(f['cases'])==138
for route in f['routes']:
 rawpath=out/(route['name']+'.json');assert sha(rawpath)==route['sha256'];raw=json.loads(rawpath.read_text());assert raw['exit_code']==0 and not raw['stderr'] and not re.search(r'commands:\d+:',raw['stdout'])
 chunks=re.split(r'"CASE ([^"]+)"',raw['stdout']);cases=[c for c in f['cases'] if c['route']==route['name']];assert len(cases)==route['cases'] and len(chunks)==2*len(cases)+1
 for c,cid,chunk in zip(cases,chunks[1::2],chunks[2::2]):
  assert c['id']==cid;statuses=re.findall(r'StatusCode: (?:\w+ \()?(-?\d+)\)?, DetailedReason: "([^"]*)"',chunk);assert len(statuses)==2
  assert c['expected']==[dict(status=int(s),detail=d) for s,d in statuses]
for row in json.loads((out/'sources.json').read_text()):assert sha(row['path'])==row['sha256']
prov=json.loads((out/'provenance.json').read_text())
for n,h in prov['binaries'].items():assert sha(Path('/tmp/pd-sched005',n))==h
safe=json.loads((out/'safe-tests.json').read_text());assert safe['exit_code']==0 and safe['stdout'].count('--- PASS:')==10
fault=json.loads((out/'fault.json').read_text());assert fault['exit_code']==0 and 'runtime.boundsError' in fault['stdout']
mut=json.loads((out/'mutation.json').read_text());assert len(mut)==6 and all(r['exit_code']==101 and r['failed']>0 for r in mut)
val=json.loads((out/'validation.json').read_text());assert len(val)==7 and all(r['exit_code']==0 for r in val);assert val[1]['passed']==3 and val[2]['passed']==14 and val[6]['passed']>0
# Metadata writes stdout and stderr separately in practice; metadata command emitted clean JSON.
metadata=json.loads((out/'validation-3.out').read_text());packages={p['name']:p for p in metadata['packages']};seen=set();pending=['astersql-pd-schedule-filter']
while pending:
 n=pending.pop()
 if n in seen or n not in packages:continue
 seen.add(n);pending.extend(d['name'] for d in packages[n]['dependencies'] if d['kind'] is None)
assert not ({'astersql-pd-server','astersql-pd-runtime'}&seen)
assert 'astersql-pd-proto' not in {d['name'] for d in packages['astersql-pd-schedule-filter']['dependencies'] if d['kind'] is None}
assert 'astersql_pd_proto' not in Path('pkg/schedule/filter/store_constraints.rs').read_text()
protocol_path=['astersql-pd-schedule-filter','astersql-pd-schedule-placement','astersql-pd-storage-endpoint','astersql-pd-proto']
for left,right in zip(protocol_path,protocol_path[1:]):assert right in {d['name'] for d in packages[left]['dependencies'] if d['kind'] is None}
old=tomllib.loads(subprocess.check_output(['git','show','c6de386d2:Cargo.lock'],text=True));new=tomllib.loads(Path('Cargo.lock').read_text());assert [p for p in old['package'] if 'source' in p]==[p for p in new['package'] if 'source' in p]
subprocess.run(['git','diff','--check'],check=True)
summary=dict(fixture_sha256=sha('docs/rust/schedule-execution/fixtures/task_008.json'),go_files_unchanged=True,go_files_checked=len(files),readonly_plans_checked=len(before['plans']),oracle_cases=len(f['cases']),oracle_source_target_results=2*len(f['cases']),complete_go_behavior_tests=10,constructor_bounds_fault=True,mutation_failures=6,rust_focused_tests=3,rust_package_tests=14,rust_workspace_tests=val[6]['passed'],rust_workspace_ignored=val[6]['ignored'],no_server_runtime_dependency=True,filter_domain_api_without_protocol_types=True,existing_transitive_protocol_path=protocol_path,registry_dependencies_unchanged=True,cargo_target_dir=val[6]['cargo_target_dir'])
(out/'evidence-verification.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary,indent=2))

for p in [Path('docs/rust/schedule-execution/task-008.md'),Path('pkg/schedule/filter/store_constraints.rs.md')]:
 for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
  if '://' not in link:assert (p.parent/link.split('#')[0]).exists(),(p,link)
