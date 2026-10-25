"""Re-derive actual Go fixture observations and audit task014's final evidence."""
import gzip,hashlib,json,re,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_014')
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
fixture=json.loads(Path('docs/rust/schedule-execution/fixtures/task_014.json').read_text())
provenance=json.loads((out/'provenance.json').read_text())
assert provenance['fixture_sha256']==sha('docs/rust/schedule-execution/fixtures/task_014.json')
for p,h in {**provenance['binaries'],**provenance['sources']}.items():assert sha(p)==h,p
assert subprocess.run(['git','merge-base','--is-ancestor',provenance['dependency_commit'],'HEAD']).returncode==0
raw=json.load(gzip.open(out/'oracle.json.gz','rt'));assert raw['exit_code']==0
ex=raw['exchanges'];assert all('Command failed:' not in e['output'] for e in ex)
pattern=r'values: \[\]string len: 4, cap: 4, \[\s*"([^"]*)",\s*"([^"]*)",\s*"([^"]*)",\s*"([^"]*)",\s*\],\s*metric:.*?valBits: (\d+),\s*valInt: (\d+)'
observations=[]
for i,e in enumerate(ex):
 if e['command']!='print hbstream.heartbeatStreamCounter.MetricVec.metricMap.metrics':continue
 rows=[]
 for address,store,kind,status,bits,value in re.findall(pattern,e['output'],re.S):
  assert bits=='0';rows.append(dict(address=address,store=store,kind=kind,status=status,value=int(value)))
 assert e['output'].count('values: []string len: 4')==len(rows)
 assert ex[i+1]['command'].endswith('.streams') and ex[i+2]['command'].endswith('.MsgLength()')
 observed=dict(metrics=sorted(rows,key=lambda r:(r['store'],r['kind'],r['status'])),bound_stores=sorted(int(x) for x in re.findall(r'\n\s*(\d+):',ex[i+1]['output'])),delivered=int(re.search(r'~r0: (\d+)',ex[i+2]['output'])[1]))
 observations.append(observed)
assert len(observations)==len(fixture['cases'])==5
for case,observed in zip(fixture['cases'],observations):
 for key,value in observed.items():assert case[key]==value,(case['id'],key)
marshal=next(e for e in ex if e['command'].startswith('call -unsafe "encoding/json".Marshal('))
nums=re.search(r'\[([0-9, ]+)\]',marshal['output'])[1]
assert fixture['keepalive']==json.loads(bytes(int(n) for n in nums.split(',')))==dict(header=dict(cluster_id=123))
lengths=[int(re.search(r'~r0: (\d+)',e['output'])[1]) for e in ex if e['command'].endswith('.MsgLength()')]
assert lengths[-2:]==[fixture['saturated_queue']['before'],fixture['saturated_queue']['after']]==[1024,1024]
assert sum(e['command'].endswith('.Close()') for e in ex)==4
assert ex[-3]['command'].endswith('.MsgLength()') and ex[-4]['command'].endswith('))')
assert fixture['descriptor']==next(e['output'] for e in ex if e['command'].endswith('metricMap.desc'))
rows=json.loads((out/'validation.json').read_text());assert len(rows)==7 and all(r['exit_code']==0 for r in rows)
assert rows[1]['passed']==5 and rows[2]['passed']==10 and rows[6]['passed']==1363
assert all(r['failed']==0 for r in rows)
mut=json.loads((out/'mutation.json').read_text());assert len(mut)==7 and all(m['exit_code']==101 and m['failed']>0 for m in mut)
for m in mut:
 log=(out/f"red-{m['mutation']}.out").read_text();assert 'panicked at' in log and 'could not compile' not in log
before=json.loads((out/'before.json').read_text())
files=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z']).decode().split('\0')
names=sorted(set(f for f in files if f and (f.endswith('.go') or Path(f).name in ['go.mod','go.sum','go.work','go.work.sum','plan.md'])))
after={f:sha(f) for f in names if Path(f).is_file()}
assert before==after,sorted(set(before)^set(after))
assert not Path('target/rust-slot-locks/slot-1.lock').exists()
p=subprocess.run(['git','diff','--check'],capture_output=True,text=True);assert p.returncode==0,p.stdout+p.stderr
for doc in ['pkg/schedule/hbstream/stream_lifecycle.rs.md','pkg/schedule/hbstream/routing.rs.md','docs/rust/schedule-execution/task-014.md']:
 for link in re.findall(r'\]\(([^)]+)\)',Path(doc).read_text()):
  if (Path(doc).parent/link).resolve()==(out/'verification.json').resolve():continue
  if '://' not in link:assert (Path(doc).parent/link.split('#')[0]).exists(),(doc,link)
result=dict(go_exit_code=raw['exit_code'],go_observations=len(observations),go_saturated_queue_size=1024,go_close_calls=4,focused_tests=rows[1]['passed'],package_tests=rows[2]['passed'],workspace_tests=rows[6]['passed'],workspace_ignored=rows[6]['ignored'],behavioral_mutations=len(mut),readonly_files=len(before),go_module_plan_hashes_unchanged=True,diff_check_exit_code=p.returncode,slot=1,cargo_target_dir=rows[1]['cargo_target_dir'],own_slot_lock_released=True)
(out/'verification.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
