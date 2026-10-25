"""Audit frozen Go outputs, real assertion failures, baselines, and read-only inputs."""
import hashlib,json,os,re,subprocess,tomllib
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_011');sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
fpath=Path('docs/rust/schedule-execution/fixtures/task_011.json');fixture=json.loads(fpath.read_text());raw=json.loads((out/'oracle.json').read_text());prov=json.loads((out/'provenance.json').read_text())
assert raw['exit_code']==0
assert [e['command'] for e in raw['exchanges']]==raw['commands']
assert ''.join([raw['stdout'].split('(dlv) ')[0]+'(dlv) ']+[e['output'] for e in raw['exchanges']])==raw['stdout']
for e in raw['exchanges']:
 assert not re.search(r'commands:\d+:|could not|has no member|can not convert',e['output'])
metric_exchanges=[i for i,e in enumerate(raw['exchanges']) if e['command']=='print filter.filterSourceCounter.MetricVec.metricMap.metrics'];assert len(metric_exchanges)==10
pattern=r'values: \[\]string len: 3, cap: 3, \[\s*"([^"]*)",\s*"([^"]*)",\s*"([^"]*)",\s*\],\s*metric:.*?valBits: (\d+),\s*valInt: (\d+)'
for case,index in zip(fixture['cases'],metric_exchanges):
 rows=[]
 for direction,j in [('source',index),('target',index+1)]:
  e=raw['exchanges'][j];assert e['command']=='print filter.filter'+direction.title()+'Counter.MetricVec.metricMap.metrics'
  matches=re.findall(pattern,e['output'],re.S);assert e['output'].count('values: []string len: 3')==len(matches)
  for scope,kind,store,bits,value in matches:
   assert bits=='0';rows.append(dict(direction=direction,scope=scope,filter=kind,store=store,value=int(value)))
 assert sorted(rows,key=lambda r:tuple(str(x) for x in r.values()))==case['metrics']
 for k,(key,count) in enumerate(zip(case['pending_keys'],case['pending'])):
  e=raw['exchanges'][index+2+k];a,f,i=key
  assert e['command'].endswith(f'.counter[{a}][{f}][{i}]')
  assert int(e['output'].split('(dlv)')[0].strip())==count
assert len(fixture['filter_names'])==26
assert fixture['descriptors']==[dict(name='pd_schedule_'+d+'_filter',help='Counter of the '+d+' filter',labels=['scope','type',d],type='COUNTER') for d in ['source','target']]
assert fixture['cases'][-1]['metrics']==[dict(direction='target',scope='lifecycle',filter='exclude-filter',store='2',value=2)]
node=[e['output'] for e in raw['exchanges'] if e['command']=='print cluster.BasicCluster.StoresInfo.stores[1].meta.NodeState'];assert len(node)==1 and re.search(r'Removed|\b3\b',node[0])
for p,h in prov['binaries'].items():assert sha(p)==h
for p,h in prov['sources'].items():assert sha(p)==h
assert sha(fpath)==prov['fixture_sha256']
assert re.search(r'func \(c \*Counter\) SetScope\(scope string\) \{\s*c.scope = scope\s*\}',Path('pkg/schedule/filter/counter.go').read_text())
safe=json.loads((out/'safe-tests.json').read_text());assert safe['exit_code']==0 and safe['stdout'].count('--- PASS:')==2
mut=json.loads((out/'mutation.json').read_text());assert len(mut)==10 and all(r['exit_code']==101 and r['failed']>0 for r in mut)
val=json.loads((out/'validation.json').read_text());assert len(val)==7 and all(r['exit_code']==0 for r in val);assert val[1]['passed']==5 and val[2]['passed']==27 and val[6]['passed']>0
for row in val:
 log=(out/row['log']).read_text();counts=[tuple(map(int,m)) for m in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored',log)];assert sum(c[0] for c in counts)==row['passed']
old=tomllib.loads(subprocess.check_output(['git','show','08a268883:Cargo.lock'],text=True));new=tomllib.loads(Path('Cargo.lock').read_text());assert [p for p in old['package'] if 'source' in p]==[p for p in new['package'] if 'source' in p]
metadata=json.JSONDecoder().raw_decode((out/'validation-3.out').read_text())[0];packages={p['name']:p for p in metadata['packages']};seen=set();pending=['astersql-pd-schedule-filter']
while pending:
 n=pending.pop()
 if n in seen or n not in packages:continue
 seen.add(n);pending.extend(d['name'] for d in packages[n]['dependencies'] if d['kind'] is None)
assert not ({'astersql-pd-server','astersql-pd-runtime'}&seen)
assert 'astersql-pd-proto' not in {d['name'] for d in packages['astersql-pd-schedule-filter']['dependencies'] if d['kind'] is None}
before=json.loads((out/'before.json').read_text());files={}
for root,ds,ns in os.walk('.'):
 ds[:]=[d for d in ds if d!='.git']
 for n in ns:
  if n.endswith('.go') or n in ('go.mod','go.sum','go.work','go.work.sum'):
   p=str(Path(root)/n);files[p]=sha(p)
changed=[p for p in sorted(set(files)|set(before['files'])) if files.get(p)!=before['files'].get(p)]
plans=[p for p,h in before['plans'].items() if sha(p)!=h]
status=subprocess.check_output(['git','status','--short','--untracked-files=all','--','*.go','**/go.mod','**/go.sum','**/go.work','**/go.work.sum','go.mod','go.sum','go.work','go.work.sum'],text=True)
(out/'go-verification.json').write_text(json.dumps(dict(before_count=len(before['files']),after_count=len(files),changed=changed,changed_plans=plans,ending_go_status=status),indent=2)+'\n');assert not changed and not plans and not status
assert not subprocess.check_output(['git','diff','--name-only',fixture['source_commit'],'--','*.go','go.mod','go.sum'],text=True)
locks=[p for p in Path('target/rust-slot-locks').glob('slot-*.lock') if (p/'owner').exists() and json.loads((p/'owner').read_text()).get('task')=='011'];assert not locks
assert Path(val[6]['cargo_target_dir']).is_dir()
subprocess.run(['git','diff','--check'],check=True)

summary=dict(source_commit=fixture['source_commit'],dependency_commit='08a268883',fixture_sha256=sha(fpath),go_cases=10,go_complete_tests=2,go_files_checked=len(files),go_unchanged=True,readonly_plans_checked=len(before['plans']),mutations_rejected=10,focused_tests=5,package_tests=27,workspace_tests=val[6]['passed'],workspace_ignored=val[6]['ignored'],cargo_slot=1,cargo_target_dir=val[6]['cargo_target_dir'],own_lock_released=True,cache_retained=True,remote_dependencies_unchanged=True)
(out/'evidence-verification.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary,indent=2))

for path in ['docs/rust/schedule-execution/task-011.md','pkg/schedule/filter/filter_metrics.rs.md']:
 p=Path(path)
 for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
  if '://' not in link:assert (p.parent/link.split('#')[0]).exists(),(path,link)
