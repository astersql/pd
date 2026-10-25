"""Audit frozen Go evidence, immutable files, test counts and dependency boundaries."""
import hashlib,json,os,re,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_006')
before=json.loads((out/'before.json').read_text());files={}
for root,dirs,names in os.walk('.'):
 dirs[:]=[d for d in dirs if d!='.git']
 for n in names:
  if n.endswith('.go') or n in ('go.mod','go.sum','go.work','go.work.sum'):
   p=str(Path(root)/n);files[p]=hashlib.sha256(Path(p).read_bytes()).hexdigest()
status=subprocess.check_output(['git','status','--short','--untracked-files=all','--','*.go','go.mod','go.sum','go.work','go.work.sum','**/go.mod','**/go.sum'],text=True)
go=dict(before_count=len(before['files']),after_count=len(files),changed=[p for p in sorted(set(files)|set(before['files'])) if files.get(p)!=before['files'].get(p)],starting_status=before['status'],ending_go_status=status,plans_checked=len(before['plans']),changed_plans=[p for p,h in before['plans'].items() if hashlib.sha256(Path(p).read_bytes()).hexdigest()!=h])
(out/'go-verification.json').write_text(json.dumps(go,indent=2)+'\n');assert not go['changed'] and not status and not go['changed_plans']
assert not subprocess.check_output(['git','diff','--name-only','836fdc7cf3ef34f7eabc9d051418995bb97efabe','--','*.go','go.mod','go.sum'],text=True)
fpath=Path('docs/rust/schedule-execution/fixtures/task_006.json');f=json.loads(fpath.read_text())
for p,h in f['raw_sha256'].items():assert hashlib.sha256((out/p).read_bytes()).hexdigest()==h
for p in ['candidates.json','comparers.json','tie.json']:
 r=json.loads((out/p).read_text());assert r['exit_code']==0 and not r['stderr'];assert not re.search(r'commands:\d+: (?!Process .* has exited with status 0)',r['stdout'])
assert len(f['comparers'])==23
for kind in ['RegionScore','Isolation']:
 results={c['expected'] for c in f['comparers'] if c['kind']==kind}
 assert results==({-1,1} if kind=='RegionScore' else {-1,0,1})
for c in f['comparers']:assert c['expected']==(1 if c['scores'][0]>c['scores'][1] else -1 if c['scores'][0]<c['scores'][1] else 0)
c=f['candidates'];replay=c['sorted'].copy()
for i,j in c['shuffle_tape']:assert 0<=j<=i;replay[i],replay[j]=replay[j],replay[i]
assert replay==c['shuffled'] and c['sorted'][c['random_index']]==c['random_pick']
for row in json.loads((out/'sources.json').read_text()):assert hashlib.sha256(Path(row['path']).read_bytes()).hexdigest()==row['sha256']
prov=json.loads((out/'provenance.json').read_text());old=json.loads(Path('docs/rust/schedule-execution/logs/task_005/provenance.json').read_text())
for n in ['filter.test','dlv']:assert prov['binaries'][n]==old['binaries'][n]
for n,h in prov['binaries'].items():assert hashlib.sha256(Path('/tmp/pd-sched005',n).read_bytes()).hexdigest()==h
safe=json.loads((out/'safe-tests.json').read_text());assert all(r['exit_code']==0 for r in safe)
assert safe[0]['stdout'].count('--- PASS:')==1 and safe[1]['stdout'].count('--- PASS:')==3
mutation=json.loads((out/'mutation.json').read_text());assert len(mutation)==4 and all(r['exit_code']==101 and r['failed']>0 for r in mutation)
validation=json.loads((out/'validation.json').read_text());assert len(validation)==7 and all(r['exit_code']==0 for r in validation)
assert validation[1]['passed']==3 and validation[2]['passed']==6 and validation[6]['passed']>0
metadata=json.loads((out/'validation-3.out').read_text());packages={p['name']:p for p in metadata['packages']};seen=set();pending=['astersql-pd-schedule-filter']
while pending:
 n=pending.pop()
 if n in seen or n not in packages:continue
 seen.add(n);pending.extend(d['name'] for d in packages[n]['dependencies'] if d['kind'] is None)
assert not ({'astersql-pd-server','astersql-pd-runtime','astersql-pd-proto'}&seen)
assert not subprocess.check_output(['git','diff','--','Cargo.toml','Cargo.lock','pkg/schedule/filter/Cargo.toml'],text=True)
summary=dict(fixture_sha256=hashlib.sha256(fpath.read_bytes()).hexdigest(),go_files_unchanged=True,read_only_plans_unchanged=True,production_dependencies_valid=True,cargo_manifests_lock_unchanged=True,go_behavior_tests=3,comparer_cases=23,region_self_tie=1,deterministic_candidate_checks=5,random_shuffle_steps=6,random_pick_cases=1,mutation_failures=4,rust_focused_tests=3,rust_package_tests=6,rust_workspace_tests=validation[6]['passed'],rust_workspace_ignored=validation[6]['ignored'],production_sha256=hashlib.sha256(Path('pkg/schedule/filter/candidates.rs').read_bytes()).hexdigest())
(out/'evidence-verification.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary,indent=2))
for p in [Path('docs/rust/schedule-execution/task-006.md'),Path('pkg/schedule/filter/candidates.rs.md')]:
 for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
  if '://' not in link:assert (p.parent/link.split('#')[0]).exists(),(p,link)
