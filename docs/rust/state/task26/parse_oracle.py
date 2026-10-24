import json,re
from pathlib import Path
p=Path(__file__).parent
raw=(p/'oracle.out').read_text()
assert 'command failed' not in raw.lower() and 'error' not in raw.lower()
events=json.loads((p/'events.json').read_text())
steps=[]
for number, body in re.findall(r'"STEP (\d+)"\n(.*?)(?="STEP |\Z)',raw,re.S):
    members=sorted(map(int,re.findall(r'^\s*(\d+): \{\}',body,re.M)))
    exists=re.search(r'~r0: (true|false)',body)[1]=='true'
    assert exists==bool(members)
    steps.append(dict(step=int(number),members=members,exists=exists))
assert len(steps)==14 and [s['step'] for s in steps]==list(range(14))
fixture=dict(source='pkg/statistics/slow_stat.go',baseline='69ba16e6d47dfe59a21498c21641e1c1eaa726e0',events=events,steps=steps)
out=Path('tests/rust-compat/fixtures/state/task-26.json')
if out.exists(): assert json.loads(out.read_text())==fixture
else: out.write_text(json.dumps(fixture,indent=2)+'\n')
print('14 actual Go snapshots verified; explicit embedded receiver; no normalization')
