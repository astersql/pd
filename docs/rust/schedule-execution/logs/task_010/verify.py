"""Audit runtime fixture, TDD, Cargo slot use, and immutable Go/plan inputs."""
import hashlib,json,os,re,subprocess,tomllib
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_010');sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest();before=json.loads((out/'before.json').read_text());files={}
for root,ds,ns in os.walk('.'):
 ds[:]=[d for d in ds if d!='.git']
 for n in ns:
  if n.endswith('.go') or n in ('go.mod','go.sum','go.work','go.work.sum'):
   p=str(Path(root)/n);files[p]=sha(p)
changed=[p for p in sorted(set(files)|set(before['files'])) if files.get(p)!=before['files'].get(p)];plans=[p for p,h in before['plans'].items() if sha(p)!=h]
status=subprocess.check_output(['git','status','--short','--untracked-files=all','--','*.go','**/go.mod','**/go.sum','**/go.work','**/go.work.sum','go.mod','go.sum','go.work','go.work.sum'],text=True)
(out/'go-verification.json').write_text(json.dumps(dict(before_count=len(before['files']),after_count=len(files),changed=changed,ending_go_status=status,changed_plans=plans),indent=2)+'\n');assert not changed and not plans and not status
assert not subprocess.check_output(['git','diff','--name-only','836fdc7cf3ef34f7eabc9d051418995bb97efabe','--','*.go','go.mod','go.sum'],text=True)
fpath=Path('docs/rust/schedule-execution/fixtures/task_010.json');f=json.loads(fpath.read_text());assert len(f['cases'])==45
for route in sorted({c['route'] for c in f['cases']}):
 raw=json.loads((out/(route+'.json')).read_text());assert raw['exit_code']==0 and not re.search(r'commands:\d+:',raw['stdout'])
 chunks=re.split(r'"CASE ([^"]+)"',raw['stdout']);cs=[c for c in f['cases'] if c['route']==route];assert len(chunks)==len(cs)*2+1
 for c,cid,chunk in zip(cs,chunks[1::2],chunks[2::2]):
  assert c['id']==cid;res=re.findall(r'StatusCode: (?:\w+ \()?(-?\d+)\)?, DetailedReason: "([^"]*)"',chunk);assert c['statuses']==[dict(code=int(s),detail=d) for s,d in res]
  assert c['bools']==[b=='true' for b in re.findall(r'~r\d+: (true|false)',chunk)]
  assert len(c['statuses'])==(2 if route=='health' else 1);assert len(c['bools'])==(2 if route=='health' else int(route.startswith('replication')))
prov=json.loads((out/'provenance.json').read_text())
for n,h in prov['binaries'].items():assert sha(Path('/tmp/pd-sched005')/n)==h
for row in prov['sources']:assert sha(row['path'])==row['sha256']
raw=json.loads((out/'fit_cache.json').read_text());s=raw['stdout'];assert raw['exit_code']==0 and not re.search(r'commands:\d+:',s)
returns=re.findall(r'StatusCode: (?:\w+ \()?(-?\d+)\)?, DetailedReason: "([^"]*)"',s);assert returns==[('0',''),('1004','')]
pointers=re.findall(r'\("\*github.com/tikv/pd/pkg/schedule/placement.RegionFit"\)\((0x[0-9a-f]+)\)',s);assert len(pointers)==2 and pointers[0]==pointers[1];assert 'placement.RegionFit nil' in s
safe=json.loads((out/'safe-tests.json').read_text());assert safe['exit_code']==0 and safe['stdout'].count('--- PASS:')==8
mut=json.loads((out/'mutation.json').read_text());assert len(mut)==10 and all(r['exit_code']==101 and r['failed']>0 for r in mut)
val=json.loads((out/'validation.json').read_text());assert len(val)==7 and all(r['exit_code']==0 for r in val);assert val[1]['passed']==4 and val[2]['passed']==22 and val[6]['passed']>0
old=tomllib.loads(subprocess.check_output(['git','show','fc1aa13f2:Cargo.lock'],text=True));new=tomllib.loads(Path('Cargo.lock').read_text());assert [p for p in old['package'] if 'source' in p]==[p for p in new['package'] if 'source' in p]
metadata=json.loads(val[3]['stdout']);packages={p['name']:p for p in metadata['packages']};seen=set();pending=['astersql-pd-schedule-filter']
while pending:
 n=pending.pop()
 if n in seen or n not in packages:continue
 seen.add(n);pending.extend(d['name'] for d in packages[n]['dependencies'] if d['kind'] is None)
assert not ({'astersql-pd-server','astersql-pd-runtime'}&seen)
assert 'astersql-pd-proto' not in {d['name'] for d in packages['astersql-pd-schedule-filter']['dependencies'] if d['kind'] is None}
assert 'astersql_pd_proto' not in Path('pkg/schedule/filter/region_health.rs').read_text()
subprocess.run(['git','diff','--check'],check=True)
owned_locks=[str(p) for p in Path('target/rust-slot-locks').glob('slot-*.lock') if (p/'owner').exists() and json.loads((p/'owner').read_text()).get('task')=='010'];assert not owned_locks
assert Path(val[6]['cargo_target_dir']).is_dir()
summary=dict(cargo_own_locks_released=True,cargo_cache_retained=True,fixture_sha256=sha(fpath),source_commit=f['source_commit'],dependency_commit='fc1aa13f2',go_files_checked=len(files),go_unchanged=True,readonly_plans_checked=len(before['plans']),oracle_cases=45,oracle_status_returns=sum(len(c['statuses']) for c in f['cases']),oracle_booleans=sum(len(c['bools']) for c in f['cases']),complete_go_tests=8,fit_cache_runtime_observed=True,mutation_failures=10,rust_focused_tests=4,rust_package_tests=22,rust_workspace_tests=val[6]['passed'],rust_workspace_ignored=val[6]['ignored'],no_server_runtime_dependency=True,registry_dependencies_unchanged=True,cargo_target_dir=val[6]['cargo_target_dir'])
(out/'evidence-verification.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary,indent=2))
for p in [Path('docs/rust/schedule-execution/task-010.md'),Path('pkg/schedule/filter/region_health.rs.md')]:
 for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
  if '://' not in link:assert (p.parent/link.split('#')[0]).exists(),(p,link)
