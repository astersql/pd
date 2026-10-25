"""Observe reject-leader using the existing checker fixture and production filter."""
import hashlib,json,re,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_007');fp=Path('docs/rust/schedule-execution/fixtures/task_007.json');f=json.loads(fp.read_text())
cmds=['break filters.go:498','continue','clearall','config max-variable-recurse 3','print store.meta.Labels','call conf.CheckLabelProperty("reject-leader", store.meta.Labels)','set store.meta.LastHeartbeat = 9000000000000000000','call -unsafe store.limiter = storelimit.NewStoreRateLimit(0.0)']
new=[]
for old in f['cases'][:9]:
 case=json.loads(json.dumps(old));case['id']='reject/'+old['id'].split('/')[1];case['store']['labels']=[{'key':'noleader','value':'true'}];case['reject_property']=True
 for k,field in [('leader','TransferLeader'),('region','MoveRegion'),('temporary','AllowTemporaryStates'),('scatter','ScatterRegion'),('fast','AllowFastFailover'),('level','OperatorLevel')]:cmds.append(f'set f.{field} = {str(case["filter"][k]).lower()}')
 cmds+=['set f.Reason = 0',f'print "CASE {case["id"]}"','call -unsafe f.Source(conf,store)','print f.Reason','call -unsafe f.Target(conf,store)','print f.Reason'];new.append(case)
cmds+=['exit'];(out/'reject.commands').write_text('\n'.join(cmds)+'\n');Path('/tmp/pd-sched005/task007reject.commands').write_text((out/'reject.commands').read_text());cmd=json.loads((out/'reject.json').read_text())['argv'];p=subprocess.run(cmd,capture_output=True,text=True);(out/'reject.json').write_text(json.dumps(dict(argv=cmd,exit_code=p.returncode,stdout=p.stdout,stderr=p.stderr),indent=2)+'\n');assert p.returncode==0 and not re.search(r'commands:\d+:',p.stdout),p.stdout
chunks=re.split(r'"CASE ([^"]+)"',p.stdout)
for case,cid,chunk in zip(new,chunks[1::2],chunks[2::2]):
 assert cid==case['id'];statuses=re.findall(r'StatusCode: (?:\w+ \()?(-?\d+)\)?, DetailedReason: "([^"]*)"',chunk);reasons=re.findall(r'^\w+ \((\d+)\)$',chunk,re.M);assert len(statuses)==len(reasons)==2
 case['expected']=[dict(status=int(s),detail=d,reason=int(r)) for (s,d),r in zip(statuses,reasons)]
f['reject_raw_sha256']=hashlib.sha256((out/'reject.json').read_bytes()).hexdigest();f['cases']=[c for c in f['cases'] if not c['id'].startswith('reject/')]+new;fp.write_text(json.dumps(f,indent=2)+'\n');print('reject frozen',len(new))
