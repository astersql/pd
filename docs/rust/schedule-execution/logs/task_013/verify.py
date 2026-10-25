"""Re-derive frozen observations from raw Go output and audit final validation."""
import hashlib,json,re,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_013')
fixture=json.loads(Path('docs/rust/schedule-execution/fixtures/task_013.json').read_text())
provenance=json.loads((out/'provenance.json').read_text())
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
assert provenance['fixture_sha256']==sha('docs/rust/schedule-execution/fixtures/task_013.json')
for p,h in {**provenance['binaries'],**provenance['sources']}.items():assert sha(p)==h,p
assert subprocess.run(['git','merge-base','--is-ancestor','e6a326a0b','HEAD']).returncode==0
raw=json.loads((out/'oracle.json').read_text());assert raw['exit_code']==0
ex=raw['exchanges'];assert all('Command failed:' not in e['output'] for e in ex)
observations=[];lengths=[]
for i,e in enumerate(ex):
 c=e['command'];s=e['output']
 if c.endswith('.MsgLength()'):lengths.append(int(re.search(r'~r0: (\d+)',s)[1]))
 if c.startswith('print ') and c.endswith('.streams'):
  msgs=[]
  for n in ex[i+1:]:
   if not n['command'].startswith('call -unsafe "encoding/json".Marshal('):break
   nums=re.search(r'\[([0-9, ]+)\]',n['output'])[1]
   msgs.append(json.loads(bytes(int(x) for x in nums.split(','))))
  observations.append(dict(queue_lengths=dict(stream1=lengths[-2],stream2=lengths[-1]),bound_stores=sorted(int(x) for x in re.findall(r'\n\s*(\d+):',s)),messages=msgs))
assert len(observations)==len(fixture['cases'])==9
for case,observed in zip(fixture['cases'],observations):
 for k,v in observed.items():assert case[k]==v,(case['id'],k)
for name in ['msgCh','streamCh']:
 s=next(e['output'] for e in ex if e['command'].startswith('print cap(') and e['command'].endswith(f'.{name})'))
 assert int(s.split('(dlv)')[0].strip())==fixture['capacities'][name]
rows=json.loads((out/'validation.json').read_text());assert len(rows)==7 and all(r['exit_code']==0 for r in rows)
assert rows[1]['passed']==3 and rows[2]['passed']==5 and rows[6]['passed']>1355
assert all(r['failed']==0 for r in rows)
mut=json.loads((out/'mutation.json').read_text());assert len(mut)==7 and all(m['exit_code']==101 and m['failed']>0 for m in mut)
for m in mut:
 s=(out/f"red-{m['mutation']}.out").read_text();assert 'panicked at' in s and 'could not compile' not in s
before=json.loads((out/'before.json').read_text())
files=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z']).decode().split('\0')
names=sorted(set(f for f in files if f and (f.endswith('.go') or Path(f).name in ['go.mod','go.sum','go.work','go.work.sum','plan.md'])))
after={f:sha(f) for f in names if Path(f).is_file()}
assert before==after, sorted(set(before)^set(after))
assert not Path('target/rust-slot-locks/slot-1.lock').exists()
p=subprocess.run(['git','diff','--check'],capture_output=True,text=True);assert p.returncode==0,p.stdout+p.stderr
for doc in ['pkg/schedule/hbstream/routing.rs.md','docs/rust/schedule-execution/task-013.md']:
 for link in re.findall(r'\]\(([^)]+)\)',Path(doc).read_text()):
  if (Path(doc).parent/link).resolve()==(out/'verification.json').resolve():continue
  if '://' not in link:assert (Path(doc).parent/link.split('#')[0]).exists(),(doc,link)
result=dict(go_exit_code=raw['exit_code'],go_fixture_cases=len(observations),go_delivered_messages=sum(len(o['messages']) for o in observations),focused_tests=rows[1]['passed'],package_tests=rows[2]['passed'],workspace_tests=rows[6]['passed'],workspace_ignored=rows[6]['ignored'],behavioral_mutations=len(mut),readonly_files=len(before),go_and_module_and_plan_hashes_unchanged=True,diff_check_exit_code=p.returncode,slot=1,cargo_target_dir=rows[1]['cargo_target_dir'],own_slot_lock_released=True)
(out/'verification.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
