"""Freeze existing Go API observations; no Go source generation or rewriting."""
import hashlib,json,re,subprocess,time
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_007');tmp=Path('/tmp/pd-sched005')
now=time.time_ns();healthy=9000000000000000000
fields={'node':'store.meta.NodeState','busy':'store.storeStats.rawStats.IsBusy','pause_in':'store.pauseLeaderTransferIn.v','pause_out':'store.pauseLeaderTransferOut.v','slow':'store.slowStoreEvicted.v','stopping':'store.stoppingStoreEvicted.v','trend':'store.slowTrendEvicted.v','sending':'store.storeStats.rawStats.SendingSnapCount','receiving':'store.storeStats.rawStats.ReceivingSnapCount','applying':'store.storeStats.rawStats.ApplyingSnapCount','pending':'store.pendingPeerCount','network':'store.networkSlowTriggers','slow_score':'store.storeStats.rawStats.SlowScore'}
base={k:False if k=='busy' else 0 for k in fields}
states=[('healthy',{}),('removed',{'node':3}),('removing',{'node':2}),('down',{'heartbeat':0}),('disconnected',{'heartbeat':now-300000000000}),('busy',{'busy':True}),('pause_in',{'pause_in':1}),('pause_out',{'pause_out':1}),('pause_both',{'pause_in':1,'pause_out':1}),('slow',{'slow':1}),('stopping',{'stopping':1}),('trend',{'trend':1}),('network_only',{'network':1}),('slow_score_only',{'slow_score':100}),('snap_equal',{'sending':64,'receiving':64}),('snap_send_over',{'sending':65}),('snap_receive_over',{'receiving':65}),('applying_only',{'applying':100}),('pending_equal',{'pending':64}),('pending_over',{'pending':65}),('removed_busy_down',{'node':3,'busy':True,'heartbeat':0}),('removing_down',{'node':2,'heartbeat':0}),('pause_slow_busy',{'pause_in':1,'slow':1,'busy':True}),('snap_pending',{'sending':65,'pending':65}),('limits',{'limit':True}),('limits_snap',{'limit':True,'sending':65})]
modes=[('leader',True,False,False,False,False,0),('region',False,True,False,False,False,0),('both',True,True,False,False,False,0),('temporary_region',False,True,True,False,False,0),('temporary_leader',True,False,True,False,False,0),('urgent',True,True,False,False,False,3),('scatter',False,True,False,True,False,0),('fast',False,True,False,False,True,0),('none',False,False,False,False,False,0)]
cmds=['break filters.go:563','continue','clearall','config max-variable-recurse 1','call conf.GetMaxStoreDownTime()','call conf.GetMaxSnapshotCount()','call conf.GetMaxPendingPeerCount()'];cases=[]
for name,changes in states:
 data=base|{'heartbeat':healthy}|changes
 for k,v in data.items():
  if k in fields:cmds.append(f'set {fields[k]} = {str(v).lower()}')
 cmds.append(f'set store.meta.LastHeartbeat = {data["heartbeat"]}')
 cmds.append('call -unsafe store.limiter = storelimit.NewStoreRateLimit(0.000000001)' if data.get('limit') else 'call -unsafe store.limiter = storelimit.NewStoreRateLimit(0.0)')
 if data.get('limit'):
  for typ in [0,1,3]:cmds.append(f'call store.limiter.Take(1000, {typ}, 0)')
 for mode,leader,region,temporary,scatter,fast,level in modes:
  for field,val in [('TransferLeader',leader),('MoveRegion',region),('AllowTemporaryStates',temporary),('ScatterRegion',scatter),('AllowFastFailover',fast),('OperatorLevel',level)]:cmds.append(f'set f.{field} = {str(val).lower()}')
  cmds.append('set f.Reason = 0')
  cid=name+'/'+mode;cmds.append(f'print "CASE {cid}"')
  for direction in ['Source','Target']:
   cmds += [f'call f.{direction}(conf, store)','print f.Reason']
  cases.append(dict(id=cid,store=data,filter=dict(leader=leader,region=region,temporary=temporary,scatter=scatter,fast=fast,level=level)))
cmds+=['exit'];(out/'oracle.commands').write_text('\n'.join(cmds)+'\n');(tmp/'task007.commands').write_text((out/'oracle.commands').read_text())
cmd=['docker','run','--rm','--cap-add','SYS_PTRACE','--security-opt','seccomp=unconfined','-v',str(tmp)+':/oracle:ro','alpine:3.22','timeout','180','/oracle/dlv','--check-go-version=false','exec','/oracle/filter.test','--allow-non-terminal-interactive','--init=/oracle/task007.commands','--','-test.run','^TestStoreStateFilter$','-test.timeout=175s']
p=subprocess.run(cmd,capture_output=True,text=True);(out/'oracle.json').write_text(json.dumps(dict(argv=cmd,exit_code=p.returncode,stdout=p.stdout,stderr=p.stderr),indent=2)+'\n');print('exit',p.returncode,'errors',re.findall(r'.*commands:\d+:.*',p.stdout)[:15])
assert p.returncode==0 and not re.search(r'commands:\d+:',p.stdout)
chunks=re.split(r'"CASE ([^"]+)"',p.stdout)
assert len(chunks)==1+2*len(cases)
for case,cid,chunk in zip(cases,chunks[1::2],chunks[2::2]):
 assert cid==case['id'];statuses=re.findall(r'StatusCode: (?:\w+ \()?(-?\d+)\)?, DetailedReason: "([^"]*)"',chunk);reasons=re.findall(r'^\w+ \((\d+)\)$',chunk,re.M)
 assert len(statuses)==2 and len(reasons)==2,(cid,chunk)
 case['expected']=[dict(status=int(s),detail=d,reason=int(r)) for (s,d),r in zip(statuses,reasons)]
fixture=dict(source_commit='836fdc7cf3ef34f7eabc9d051418995bb97efabe',mode='classic/default',now_unix_nanos=now,max_down_nanos=1800000000000,max_snapshot=64,max_pending=64,raw_sha256=hashlib.sha256((out/'oracle.json').read_bytes()).hexdigest(),cases=cases)
Path('docs/rust/schedule-execution/fixtures/task_007.json').write_text(json.dumps(fixture,indent=2)+'\n');print('frozen',len(cases),'cases')
