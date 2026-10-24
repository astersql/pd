import json,re,struct
from pathlib import Path
p=Path(__file__).parent
s=(p/'oracle.out').read_text()
assert '--- PASS: TestStoreStatistics' in s
assert not re.search(r'/oracle/oracle.commands:\d+: (?!Process .* exited with status 0)',s)
status={}
for m in re.finditer(r'"(STORE \d+|SLOW|DISCONNECTED|UNHEALTHY|DOWN|SERVING|REMOVED|UNKNOWN)"\n(.*?)(?=\n"|\Z)',s,re.S):
 status[m[1]]=dict((k,float(v)) for k,v in re.findall(r'"(store_\w+)": (\d+)',m[2]))
 assert len(status[m[1]])==12
assert len(status)==17
samples={}
for key,end in [('PLACEMENT','METRIC CAPACITY'),('METRIC CAPACITY',None)]:
 part=s.split('"'+key+'"\n')[1]
 if end:part=part.split('"'+end+'"\n')[0]
 values=[]
 for m in re.finditer(r'values: \[\]string len: 3, cap: 3, \[(.*?)\],\s+metric: .*?valBits: (\d+)',part,re.S):
  labels=re.findall(r'"([^"]*)"',m[1]); assert len(labels)==3
  value=struct.unpack('d',struct.pack('Q',int(m[2])))[0]
  values.append(dict(labels=labels,value=value))
 samples[key]=sorted(values,key=lambda v:v['labels'])
assert len(samples['PLACEMENT'])==18
assert len(samples['METRIC CAPACITY'])==162
out=dict(source_commit='0a76af3b4',status=status,placement=samples['PLACEMENT'],cluster=samples['METRIC CAPACITY'])
Path('tests/rust-compat/fixtures/state/task-27.json').write_text(json.dumps(out,indent=2)+'\n')
print('17 status snapshots; 18 placement and 162 cluster metric samples extracted')
