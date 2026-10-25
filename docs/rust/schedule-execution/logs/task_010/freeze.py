"""Freeze production returns from existing immutable Go binaries; no Go compilation."""
import subprocess,json,re,hashlib
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_010');tmp=Path('/tmp/pd-sched005');cases=[]
def run(name,cmds,test):
 text='\n'.join(cmds+['exit'])+'\n';(out/(name+'.commands')).write_text(text);(tmp/('task010-'+name+'.commands')).write_text(text)
 argv=['docker','run','--rm','--cap-add','SYS_PTRACE','--security-opt','seccomp=unconfined','-v',str(tmp)+':/oracle:ro','alpine:3.22','timeout','180','/oracle/dlv','--check-go-version=false','exec','/oracle/filter.test','--allow-non-terminal-interactive','--init=/oracle/task010-'+name+'.commands','--','-test.run',test,'-test.timeout=175s']
 p=subprocess.run(argv,capture_output=True,text=True);(out/(name+'.json')).write_text(json.dumps(dict(argv=argv,exit_code=p.returncode,stdout=p.stdout,stderr=p.stderr),indent=2)+'\n');assert p.returncode==0 and not re.search(r'commands:\d+:',p.stdout),(name,p.stdout[-5000:])
 chunks=re.split(r'"CASE ([^"]+)"',p.stdout);rows=[c for c in cases if c['route']==name];assert len(chunks)==len(rows)*2+1
 for c,cid,chunk in zip(rows,chunks[1::2],chunks[2::2]):
  assert cid==c['id'];statuses=re.findall(r'StatusCode: (?:\w+ \()?(-?\d+)\)?, DetailedReason: "([^"]*)"',chunk)
  bools=re.findall(r'~r\d+: (true|false)',chunk)
  c['statuses']=[dict(code=int(s),detail=d) for s,d in statuses];c['bools']=[x=='true' for x in bools]
 print(name,len(rows),flush=True)
def observe(cmds,route,expressions,**inputs):
 cid=route+'/'+str(len(cases));cases.append(dict(id=cid,route=route,**inputs));cmds.append(f'print "CASE {cid}"');cmds.extend('call -unsafe '+x for x in expressions)
cmds=['break healthy_test.go:81','continue','clearall','config max-variable-recurse 1']
for i,(n,learner,pending,down) in enumerate([(3,False,False,False),(3,False,True,False),(3,True,False,False),(3,False,False,True),(2,False,False,False),(4,True,False,False)]):
 for rules in [False,True]:
  cmds.append('call -unsafe tc.SetEnablePlacementRules('+str(rules).lower()+')')
  observe(cmds,'health',[f'IsRegionHealthy(testCases[{i}].region)',f'IsRegionHealthyAllowPending(testCases[{i}].region)',f'NewRegionPendingFilter().Select(testCases[{i}].region)',f'NewRegionDownFilter().Select(testCases[{i}].region)'],peers=n,learner=learner,pending=pending,down=down,rules=rules)
run('health',cmds,'^TestIsRegionHealthy$')
for i,(n,learner,pending,down) in enumerate([(3,False,False,False),(3,False,True,False),(3,True,False,False),(3,False,False,True),(2,False,False,False),(4,True,False,False)]):
 for rules in [False,True]:
  route='replication'+str(i)+str(rules)
  cmds=['break healthy.go:46']+['continue']*(2*i+1+int(rules))+['clearall','config max-variable-recurse 1']
  observe(cmds,route,['IsRegionReplicated(cluster,region)','NewRegionReplicatedFilter(cluster).Select(region)'],peers=n,learner=learner,pending=pending,down=down,rules=rules)
  run(route,cmds,'^TestIsRegionHealthy$')
cmds=['break region_filters_test.go:102','continue','clearall','config max-variable-recurse 1']
for size in [0,1,2,30]:
 cmds.append(f'set region.approximateSize = {size}');observe(cmds,'empty_small',['filter.Select(region)'],size=size,total=0)
run('empty_small',cmds,'^TestRegionEmptyFilter$')
cmds=['break region_filters_test.go:118','continue','clearall','config max-variable-recurse 1']
for size in [0,1,2,30]:
 cmds.append(f'set region.approximateSize = {size}');observe(cmds,'empty_large',['filter.Select(region)'],size=size,total=100)
run('empty_large',cmds,'^TestRegionEmptyFilter$')
cmds=['break region_filters_test.go:130','continue','clearall','config max-variable-recurse 1']
for store in [0,1,2,3,99]:observe(cmds,'witness',[f'NewRegionWitnessFilter({store}).Select(region)'],store=store)
run('witness',cmds,'^TestRegionWitnessFilter$')
cmds=['break filters_test.go:235','continue','clearall','config max-variable-recurse 1'];observe(cmds,'snapshot_open',['snapshotFilter.Select(region)'],limited=False)
run('snapshot_open',cmds,'^TestSendStateFilter$')
cmds=['break filters_test.go:239','continue','clearall','config max-variable-recurse 1'];observe(cmds,'snapshot_limited',['snapshotFilter.Select(region)'],limited=True)
run('snapshot_limited',cmds,'^TestSendStateFilter$')
cmds=['break region_filters_test.go:166','continue','clearall','config max-variable-recurse 1']
observe(cmds,'affinity',['filter.Select(affinityRegion)'],start='b',end='y');observe(cmds,'affinity',['filter.Select(nonAffinityRegion)'],start='za',end='zz')
run('affinity',cmds,'^TestAffinitySelect$')
Path('docs/rust/schedule-execution/fixtures/task_010.json').write_text(json.dumps(dict(source_commit='836fdc7cf3ef34f7eabc9d051418995bb97efabe',cases=cases,binaries={n:hashlib.sha256((tmp/n).read_bytes()).hexdigest() for n in ['filter.test','dlv']}),indent=2)+'\n')
