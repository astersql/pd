"""Freeze real Go production metrics from existing binaries; never compile Go."""
import subprocess,os,select,time,json,re,hashlib
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_013');tmp=Path('/tmp/pd-sched003');commands=[];transcript=[];exchanges=[]
argv=['docker','run','--rm','-i','--cap-add','SYS_PTRACE','--security-opt','seccomp=unconfined','-v',str(tmp)+':/oracle:ro','alpine:3.22','timeout','300','/oracle/dlv','--check-go-version=false','exec','/oracle/cluster.test','--allow-non-terminal-interactive','--','-test.run','^TestSetOfflineStore$','-test.timeout=295s']
p=subprocess.Popen(argv,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
def read():
 b=b'';end=time.time()+15
 while time.time()<end:
  if select.select([p.stdout],[],[],1)[0]:
   x=os.read(p.stdout.fileno(),65536)
   if not x:break
   b+=x
   if b'(dlv) ' in b:break
 s=b.decode();transcript.append(s);return s
def cmd(s):
 print(s,flush=True);commands.append(s);p.stdin.write((s+'\n').encode());p.stdin.flush();s=read();exchanges.append(dict(command=commands[-1],output=s))
 assert not any(x in s for x in ['could not','has no member','not allowed','can not convert','not found','too many arguments','not enough arguments','Command failed:']),s
 return s


def alloc(t):
 a=re.search(r'unsafe.Pointer\((0x[0-9a-f]+)\)',cmd('call runtime.mallocgc(4096,nil,true)'))[1]
 return f'(*"{t}")({a})'
def newhs(run):
 s=cmd(f'call -unsafe hbstream.NewTestHeartbeatStreams(cluster.ctx,cluster.ruleManager.storeSetInformer,{str(run).lower()})')
 a=re.search(r'hbstream.HeartbeatStreams"\)\((0x[0-9a-f]+)\)',s)[1]
 return f'(*hbstream.HeartbeatStreams)({a})'
cases=[]
capacities={}
try:
 read();cmd('break cluster_test.go:300');cmd('continue');cmd('clearall');cmd('config max-variable-recurse 5');cmd('config max-array-values 1100');cmd('config max-string-len 10000');cmd('call "runtime/debug".SetGCPercent(-1)')
 hs=newhs(True);cmd(f'set {hs}.clusterID = 123');q1=newhs(False);q2=newhs(False)
 # Existing mock transport only: route sends into real bounded Go channels.
 streams=[]
 for q in [q1,q2]:
  a=alloc('github.com/tikv/pd/pkg/mock/mockhbstream.HeartbeatStream');cmd(f'set {a}.ch = {q}.msgCh');iface=alloc('github.com/tikv/pd/pkg/schedule/hbstream.HeartbeatStream')
  addr=re.search(r'\((0x[0-9a-f]+)\)$',iface)[1]
  cmd(f'set (*[2]uintptr)({addr})[0] = 0x1f192e0')
  data=re.search(r'\((0x[0-9a-f]+)\)$',a)[1]
  cmd(f'set (*[2]uintptr)({addr})[1] = (*[1]uintptr)({data})[0]')
  streams.append((a,iface))
 (s1,i1),(s2,i2)=streams
 sleeper=alloc('github.com/tikv/pd/pkg/mock/mockhbstream.HeartbeatStream')
 peer=alloc('github.com/pingcap/kvproto/pkg/metapb.Peer');cmd(f'set {peer}.Id = 1');cmd(f'set {peer}.StoreId = 1')
 region=alloc('github.com/tikv/pd/pkg/core.RegionInfo');meta=alloc('github.com/pingcap/kvproto/pkg/metapb.Region');cmd(f'set {meta}.Id = 55');cmd(f'set {region}.meta = {meta}');cmd(f'set {region}.leader = {peer}')
 op=alloc('github.com/tikv/pd/pkg/schedule/hbstream.Operation')
 for name in ['msgCh','streamCh']:
  raw=cmd(f'print cap({hs}.{name})');capacities[name]=int(raw.split('(dlv)')[0].strip())
 def wait():cmd(f'call -unsafe {sleeper}.Recv()')
 def bind(store,stream):cmd(f'call -unsafe {hs}.BindStream({store},*{i1 if stream==s1 else i2})');wait()
 def send(rid=55,store=1):
  cmd(f'set {meta}.Id = {rid}');cmd(f'set {peer}.StoreId = {store}');cmd(f'call -unsafe {hs}.SendMsg({region},{op})');wait()
 def observe(name,ops,queues):
  states={}
  for i,q in enumerate([q1,q2]):
   raw=cmd(f'call {q}.MsgLength()');n=int(re.search(r'~r0: (\d+)',raw)[1]);states[f'stream{i+1}']=n
  raw=cmd(f'print {hs}.streams');ids=[int(x) for x in re.findall(r'\n\s*(\d+):',raw)]
  # channel ring index is inspected only where payload content matters.
  observed=[]
  for q,indices in queues:
   for ix in indices:
    raw=cmd(f'call -unsafe "encoding/json".Marshal({q}.msgCh.buf[{ix}])')
    nums=re.search(r'\[([0-9, ]+)\]',raw)[1];observed.append(json.loads(bytes(int(n) for n in nums.split(','))))
  cases.append(dict(id=name,operations=ops,queue_lengths=states,bound_stores=sorted(ids),messages=observed));print('CASE',cases[-1],flush=True)
 send();observe('unbound',[['send',55,1]],[])
 bind(1,s1);send();observe('bound',[['bind',1,1],['send',55,1]],[(q1,[0])])
 bind(1,s2);send(56);observe('replaced',[['bind',1,2],['send',56,1]],[(q2,[0])])
 bind(99,s1);send(57,99);observe('missing',[['bind',99,1],['send',57,99]],[])
 cmd('set cluster.BasicCluster.StoresInfo.stores[2].meta.NodeState = 3');bind(2,s1);send(58,2);observe('removed',[['remove',2],['bind',2,1],['send',58,2]],[])
 for i in range(3):send(60+i)
 observe('ordered',[['send',60+i,1] for i in range(3)],[(q2,[1,2,3])])
 # nil transport channel is a controlled disconnect; existing Send returns timeout.
 cmd(f'set {s1}.ch = nil');cmd(f'set (*[2]uintptr)({re.search(r"\((0x[0-9a-f]+)\)$",i1)[1]})[1] = 0');bind(1,s1);send(70);[wait() for _ in range(110)];observe('send_error',[['disconnect',1],['bind',1,1],['send',70,1]],[])
 send(71);observe('error_unbound',[['send',71,1]],[])
 bind(1,s2);send(72);observe('recovered',[['bind',1,2],['send',72,1]],[(q2,[4])])
 for q in [hs,q1,q2]:cmd(f'call -unsafe {q}.Close()')
 cmd('exit');code=p.wait();assert code==0
 Path('docs/rust/schedule-execution/fixtures/task_013.json').write_text(json.dumps(dict(source_commit='836fdc7cf3ef34f7eabc9d051418995bb97efabe',cases=cases,capacities=capacities),indent=2)+'\n')
finally:
 if p.poll() is None:p.terminate();p.wait()
 (out/'oracle.json').write_text(json.dumps(dict(argv=argv,exit_code=p.returncode,exchanges=exchanges),indent=2)+'\n')
 (out/'oracle.commands').write_text('\n'.join(commands)+'\n')
