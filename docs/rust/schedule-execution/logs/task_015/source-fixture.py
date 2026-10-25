"""Freeze source-derived expectations; this script never runs Go or the Rust SUT."""
import hashlib,json,re,subprocess
from pathlib import Path
root=Path('.')
source_commit='836fdc7cf3ef34f7eabc9d051418995bb97efabe'
paths=['pkg/schedule/operator/'+p for p in ['kind.go','status.go','status_tracker.go','status_test.go','status_tracker_test.go']]
sources={p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in paths}
for p in paths:assert Path(p).read_bytes()==subprocess.check_output(['git','show',source_commit+':'+p])
kind=Path(paths[0]).read_text();status=Path(paths[1]).read_text()
# Read declarations and maps from Go; do not derive expectations from Rust.
block=kind.split('const (',1)[1].split('\n)',1)[0]
symbols=re.findall(r'^\s*(Op\w+)(?:\s+OpKind = 1 << iota)?\s*$',block,re.M)
assert len(symbols)==12
bits={symbol:1<<i for i,symbol in enumerate(symbols)}
parse_names=dict((name,bits[symbol]) for name,symbol in re.findall(r'"([^"\n]+)":\s*(Op\w+)',kind.split('var nameToFlag =',1)[1].split('\n}',1)[0]))
display_names={bits[symbol]:name for symbol,name in re.findall(r'(Op\w+):\s*"([^"\n]+)"',kind.split('var flagToName =',1)[1].split('\n}',1)[0])}
inputs=['admin','affinity','merge','range','replica','split','hot-region','region','leader','witness-leader','split-scatter','admin,leader','region,admin,region','witness','', 'leader, region','affinity,split-scatter','bogus','leader,','Leader']
cases=[]
for input in inputs:
 value=0;error=None
 for name in input.split(','):
  if name not in parse_names:error='unknown flag name: '+name;break
  value|=parse_names[name]
 cases.append(dict(input=input,error=error) if error is not None else dict(input=input,bits=value))
status_names=['CREATED','STARTED','SUCCESS','CANCELED','REPLACED','EXPIRED','TIMEOUT']
strings=dict(re.findall(r'(\w+):\s*"(\w+)"',status.split('var statusString =',1)[1].split('\n}',1)[0]))
trans=status.split('var validTrans =',1)[1].split('var statusString',1)[0]
allowed=set()
for src,body in re.findall(r'(\w+):\s*\{([^}]+)\}',trans):
 for dst in re.findall(r'(\w+):\s*true',body):allowed.add((status_names.index(src),status_names.index(dst)))
assert len(allowed)==7
fixture=dict(evidence_kind='source_assertion',go_execution=False,source_commit=source_commit,sources=sources,
 authorization='User waived required Go runtime oracle; source contracts plus actual Rust tests suffice.',
 kinds=cases,
 kind_strings=[dict(bits=b,output=','.join(name for bit,name in sorted(display_names.items()) if b&bit) or 'unknown') for b in [0,1,2,4,8,16,32,64,128,256,512,1024,2048,4096,8191]],
 statuses=[dict(input=i,end=2<=i<7,name=strings[status_names[i]] if i<7 else 'Unknown') for i in [0,1,2,3,4,5,6,7,100,4294967295]],
 transitions=[dict(**{'from':src,'to':dst},changed=(src,dst) in allowed,after=dst if (src,dst) in allowed else src) for src in range(7) for dst in [0,1,2,3,4,5,6,7,8,4294967295]],
 merge=[dict(input=v,has=bool(v),id=result) for v,result in [('',0),('42',42),('0',0),('+42',0),('-1',0),('abc',0),('18446744073709551615',18446744073709551615),('18446744073709551616',0),(' 42',0),('00042',42)]],
 merge_log='{"related merge region":"00042"}',escaped_log='{"escape":"\\u003c\\u003e\\u0026\\u2028\\u2029","related merge region":"00042"}')
Path('docs/rust/schedule-execution/fixtures/task_015.json').write_text(json.dumps(fixture,indent=2)+'\n')
print('source contract cases:', {k:len(fixture[k]) for k in ['kinds','kind_strings','statuses','transitions','merge']})
