"""Extract observed fields from the frozen Go debugger transcript."""
import json,re,hashlib
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_005');raw=json.loads((out/'oracle.json').read_text());s=raw['stdout'];assert raw['exit_code']==0 and 'commands:' not in s
parts=s.split('"CASE ');intro=parts[0]
ptrs=re.findall(r'StoreInfo"\)\((0x[0-9a-f]+)\)',intro);ids=list(map(int,re.findall(r'~r0: (\d+)',intro)));assert len(ptrs)==len(ids)==4
mapping=dict(zip(ptrs,ids));cases=json.loads((out/'case-inputs.json').read_text())
for c,part in zip(cases,parts[1:]):
 assert part.startswith(c['name']+'"')
 selection=part.split('~r0: []*')[1].split('\n\n')[0]
 c['input_ids']=ids[1:2] if c['removed'] else ids
 c['selected']=[mapping[x] for x in re.findall(r'StoreInfo"\)\((0x[0-9a-f]+)\)',selection)]
 c['source_exclusions']=[ids[0]];c['target_exclusions']=[ids[1]];c['plans']=[]
 if c['filters']:
  values=list(map(int,re.findall(r'~r0: (\d+)',part)));step,resource=values[-2:]
  status,reason=re.findall(r'StatusCode: (\d+), DetailedReason: "([^"]*)"',part)[-1]
  c['plans']=[dict(step=step,resource=resource,status=int(status),reason=reason)]
 c['rust_parity']=c['name']!='first_state'
f=dict(source_commit='836fdc7cf3ef34f7eabc9d051418995bb97efabe',raw_output_sha256=hashlib.sha256((out/'oracle.json').read_bytes()).hexdigest(),normalization='map each returned StoreInfo pointer to its observed input GetID; preserve input and result order; status codes/reasons/step/resource unchanged; remove debugger addresses and logging timestamps only',cases=cases)
Path('docs/rust/schedule-execution/fixtures/task_005.json').write_text(json.dumps(f,indent=2)+'\n')
