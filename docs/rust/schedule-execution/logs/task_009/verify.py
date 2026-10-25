"""Reparse frozen returns and audit immutable Go inputs, plans and verification."""
import hashlib,json,os,re,subprocess,tomllib
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_009');before=json.loads((out/'before.json').read_text());sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest();files={}
for root,ds,ns in os.walk('.'):
 ds[:]=[d for d in ds if d!='.git']
 for n in ns:
  if n.endswith('.go') or n in ('go.mod','go.sum','go.work','go.work.sum'):
   p=str(Path(root)/n);files[p]=sha(p)
changed=[p for p in sorted(set(files)|set(before['files'])) if files.get(p)!=before['files'].get(p)];plans=[p for p,h in before['plans'].items() if sha(p)!=h]
status=subprocess.check_output(['git','status','--short','--untracked-files=all','--','*.go','**/go.mod','**/go.sum','**/go.work','**/go.work.sum','go.mod','go.sum','go.work','go.work.sum'],text=True)
(out/'go-verification.json').write_text(json.dumps(dict(before_count=len(before['files']),after_count=len(files),changed=changed,ending_go_status=status,changed_plans=plans),indent=2)+'\n');assert not changed and not plans and not status
assert not subprocess.check_output(['git','diff','--name-only','836fdc7cf3ef34f7eabc9d051418995bb97efabe','--','*.go','go.mod','go.sum'],text=True)
f=json.loads(Path('docs/rust/schedule-execution/fixtures/task_009.json').read_text());assert len(f['cases'])==129
for route in f['routes']:
 p=out/(route['name']+'.json');assert sha(p)==route['sha256'];raw=json.loads(p.read_text());assert raw['exit_code']==0 and not raw['stderr'] and not re.search(r'commands:\d+:',raw['stdout'])
 chunks=re.split(r'"CASE ([^"]+)"',raw['stdout']);cs=[c for c in f['cases'] if c['route']==route['name']];assert len(cs)==route['cases'] and len(chunks)==len(cs)*2+1
 for c,cid,chunk in zip(cs,chunks[1::2],chunks[2::2]):
  assert c['id']==cid;res=re.findall(r'StatusCode: (?:\w+ \()?(-?\d+)\)?, DetailedReason: "([^"]*)"',chunk);assert len(res)==2;assert c['expected']==[dict(status=int(s),detail=d) for s,d in res]
prov=json.loads((out/'provenance.json').read_text())
for n,h in prov['binaries'].items():assert sha(Path('/tmp/pd-sched005')/n)==h
observed={pid:int(store) for pid,store,_ in re.findall(r'\*\{Id: (\d+), StoreId: (\d+), Role: PeerRole_\w+ \(\d+\), IsWitness: (true|false)\}',json.loads((out/'witness.json').read_text())['stdout'])}
assert prov['witness_peer_id_mapping']==observed and sorted(observed.values())==[1,2,3,4]
sources=['pkg/schedule/filter/filters.go','pkg/schedule/filter/filters_test.go','pkg/schedule/filter/candidates_test.go','pkg/schedule/schedulers/balance_witness.go','pkg/schedule/schedulers/balance_witness_test.go','pkg/schedule/schedulers/scheduler_test.go','pkg/schedule/schedulers/balance_leader_test.go','pkg/utils/testutil/leak.go','pkg/schedule/placement/fit.go']
rows=[dict(path=p,sha256=sha(p)) for p in sources];(out/'sources.json').write_text(json.dumps(rows,indent=2)+'\n')
safe=json.loads((out/'safe-tests.json').read_text());assert safe['exit_code']==0 and safe['stdout'].count('--- PASS:')==6
raw=json.loads((out/'temporary.json').read_text());assert raw['exit_code']==0 and not re.search(r'commands:\d+:',raw['stdout']);s=raw['stdout'];assert s.split('"BEFORE"')[1].split('"TEMPORARY"')[0].count('IsWitness: true')==1 and s.split('"AFTER"')[1].count('IsWitness: true')==1
assert 'Id: 7, StoreId: 7, Role: PeerRole_Voter (0), IsWitness: false' in s.split('"TEMPORARY"')[1].split('"AFTER"')[0]
fault=json.loads((out/'fault.json').read_text());assert fault['exit_code']==0 and 'makeslice: cap out of range' in fault['stdout']
red=(out/'red-empty.out').read_text();assert '3 passed; 1 failed;' in red and 'command_exit_code=101' in red
mut=json.loads((out/'mutation.json').read_text());assert len(mut)==8 and all(r['exit_code']==101 and r['failed']>0 for r in mut)
val=json.loads((out/'validation.json').read_text());assert len(val)==7 and all(r['exit_code']==0 for r in val);assert val[1]['passed']==4 and val[2]['passed']==18 and val[6]['passed']>0
old=tomllib.loads(subprocess.check_output(['git','show','0992b0b20:Cargo.lock'],text=True));new=tomllib.loads(Path('Cargo.lock').read_text());assert [p for p in old['package'] if 'source' in p]==[p for p in new['package'] if 'source' in p]
metadata=json.loads(val[3]['stdout']);packages={p['name']:p for p in metadata['packages']};seen=set();pending=['astersql-pd-schedule-filter']
while pending:
 n=pending.pop()
 if n in seen or n not in packages:continue
 seen.add(n);pending.extend(d['name'] for d in packages[n]['dependencies'] if d['kind'] is None)
assert not ({'astersql-pd-server','astersql-pd-runtime'}&seen)
assert 'astersql-pd-proto' not in {d['name'] for d in packages['astersql-pd-schedule-filter']['dependencies'] if d['kind'] is None}
assert 'astersql_pd_proto' not in Path('pkg/schedule/filter/placement_guard.rs').read_text()
subprocess.run(['git','diff','--check'],check=True)
owned_locks=[str(p) for p in Path('target/rust-slot-locks').glob('slot-*.lock') if (p/'owner').exists() and json.loads((p/'owner').read_text()).get('task')=='009'];assert not owned_locks
assert Path(val[6]['cargo_target_dir']).is_dir()
summary=dict(cargo_own_locks_released=True,cargo_cache_retained=True,fixture_sha256=sha('docs/rust/schedule-execution/fixtures/task_009.json'),source_commit=f['source_commit'],dependency_commit='0992b0b20',go_files_checked=len(files),go_unchanged=True,readonly_plans_checked=len(before['plans']),oracle_cases=129,oracle_status_returns=258,complete_go_tests=6,temporary_region_runtime_observed=True,mutation_failures=8,rust_focused_tests=4,rust_package_tests=18,rust_workspace_tests=val[6]['passed'],rust_workspace_ignored=val[6]['ignored'],no_server_runtime_dependency=True,filter_domain_api_without_protocol_types=True,registry_dependencies_unchanged=True,cargo_target_dir=val[6]['cargo_target_dir'])
(out/'evidence-verification.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary,indent=2))
for p in [Path('docs/rust/schedule-execution/task-009.md'),Path('pkg/schedule/filter/placement_guard.rs.md')]:
 for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
  if '://' not in link:assert (p.parent/link.split('#')[0]).exists(),(p,link)
