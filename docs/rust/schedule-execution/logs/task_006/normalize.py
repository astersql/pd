"""Extract observed Go branches and samples. Never reads Rust implementation."""
import re,json,hashlib
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_006')
raw=json.loads((out/'comparers.json').read_text())['stdout']
cases=[]
def scalar(block,field):return json.loads(re.search(r'\n\t'+re.escape(field)+r': ([^\n]+)',block)[1])
def labels(block,field):
 text=re.search(r'\n\t'+re.escape(field)+r': .*?\[(.*?)\n\t\]',block,re.S)[1]
 return dict(re.findall(r'Key: "([^"]*)", Value: "([^"]*)"',text))
def store(block,prefix,full=True):
 v=dict(id=scalar(block,prefix+'.meta.Id'),labels=labels(block,prefix+'.meta.Labels'))
 if full:
  for name,field in [('size','regionSize'),('weight','regionWeight'),('capacity','storeStats.rawStats.Capacity'),('available','storeStats.rawStats.Available'),('used','storeStats.rawStats.UsedSize')]:v[name]=scalar(block,prefix+'.'+field)
 return v
for block in raw.split('> goroutine(')[1:]:
 m=re.search(r'filter\.(RegionScore|Isolation)Comparer.func1',block)
 if not m:continue
 ret=re.search(r'"RETURN (-?\d+)":',block)
 if not ret:continue
 v=dict(kind=m[1],a=store(block,'a'),b=store(block,'b'),scores=[scalar(block,'sa'),scalar(block,'sb')],expected=int(ret[1]))
 if m[1]=='Isolation':
  v['labels']=json.loads('['+re.search(r'locationLabels: .*?cap: \d+, \[([^\]]*)\]',block)[1]+']')
  v['region_stores']=[store(block,f'regionStores[{i}]',False) for i in range(2)]
 if v not in cases:cases.append(v)
raw2=json.loads((out/'candidates.json').read_text())['stdout']
lines={}
for block in raw2.split('> goroutine(')[1:]:
 m=re.search(r'"LINE (\d+)":',block)
 if m:lines[int(m[1])]=block
outputs={str(n):list(map(int,re.findall(r'cs.Stores\[\d+\].meta.Id: (\d+)',b))) for n,b in lines.items()}
tape=[list(map(int,m)) for m in re.findall(r'\n\ti: (\d+)\n\tj: (\d+)',raw2)]
assert [x[0] for x in tape]==list(range(6,0,-1))
fixture=dict(source_commit='836fdc7cf3ef34f7eabc9d051418995bb97efabe',mode='classic/default',normalization='Keep all domain fields/order. Omit process addresses/log timestamps. Comparer expected is the actually executed return branch, scores recorded separately. Shuffle tape contains actual Go bounded samples; replay at RNG boundary, no PRNG equivalence claimed.',raw_sha256={f:hashlib.sha256((out/f).read_bytes()).hexdigest() for f in ['comparers.json','candidates.json']},region_config=dict(version='v2',high=0.7,low=0.8),comparers=cases,candidates=dict(input=[1,3,5,7,6,2,4],sorted=outputs['97'],minimum=scalar(lines[94],'minStore.meta.Id'),maximum=scalar(lines[94],'maxStore.meta.Id'),shuffled=outputs['103'],shuffle_tape=tape,random_pick=scalar(lines[106],'store.meta.Id'),random_index=scalar(lines[106],'store.meta.Id')-1,top_input=[10,15,23,20,33,32,31],top_desc=outputs['111'],top_asc=outputs['115']))
tie=json.loads((out/'tie.json').read_text()); assert '~r0: 0' in tie['stdout']; size=int(re.search(r'\n(\d+)\n> github',tie['stdout'])[1]); fixture['region_tie']=dict(a=next(c['a'] for c in cases if c['kind']=='RegionScore' and c['a']['size']==size),expected=0); fixture['raw_sha256']['tie.json']=hashlib.sha256((out/'tie.json').read_bytes()).hexdigest()
Path('docs/rust/schedule-execution/fixtures/task_006.json').write_text(json.dumps(fixture,indent=2)+'\n')
print('comparers',len(cases),{k:sorted({c['expected'] for c in cases if c['kind']==k}) for k in ['RegionScore','Isolation']},'tape',tape)
