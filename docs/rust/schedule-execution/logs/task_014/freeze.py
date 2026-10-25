"""Freeze real Go production metrics from existing binaries; never compile Go."""
import subprocess,os,select,time,json,re,hashlib,gzip
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_014');tmp=Path('/tmp/pd-sched003');commands=[];transcript=[];exchanges=[]
argv=['docker','run','--rm','-i','--cap-add','SYS_PTRACE','--security-opt','seccomp=unconfined','-v',str(tmp)+':/oracle:ro','alpine:3.22','timeout','900','/oracle/dlv','--check-go-version=false','exec','/oracle/cluster.test','--allow-non-terminal-interactive','--','-test.run','^TestSetOfflineStore$','-test.timeout=895s']
p=subprocess.Popen(argv,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
def read():
 b=b'';end=time.time()+170
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
def wait():cmd(f'call -unsafe {sleeper}.Recv()')
def observe(name):
 raw=cmd('print hbstream.heartbeatStreamCounter.MetricVec.metricMap.metrics')
 pattern=r'values: \[\]string len: 4, cap: 4, \[\s*"([^"]*)",\s*"([^"]*)",\s*"([^"]*)",\s*"([^"]*)",\s*\],\s*metric:.*?valBits: (\d+),\s*valInt: (\d+)'
 rows=[]
 for address,store,kind,status,bits,value in re.findall(pattern,raw,re.S):
  assert bits=='0';rows.append(dict(address=address,store=store,kind=kind,status=status,value=int(value)))
 assert raw.count('values: []string len: 4')==len(rows),raw
 raw=cmd(f'print {hs}.streams');ids=sorted(int(x) for x in re.findall(r'\n\s*(\d+):',raw))
 length=int(re.search(r'~r0: (\d+)',cmd(f'call {q}.MsgLength()'))[1])
 cases.append(dict(id=name,metrics=sorted(rows,key=lambda r:(r['store'],r['kind'],r['status'])),bound_stores=ids,delivered=length))
 print('CASE',cases[-1],flush=True)
try:
 read();cmd('break cluster_test.go:300');cmd('continue');cmd('clearall');cmd('config max-variable-recurse 5');cmd('config max-array-values 1100');cmd('config max-string-len 10000');cmd('call "runtime/debug".SetGCPercent(-1)')
 cmd('call -unsafe hbstream.ResetHeartbeatStreamMetrics()')
 hs=newhs(True);cmd(f'set {hs}.clusterID = 123');q=newhs(False)
 sleeper=alloc('github.com/tikv/pd/pkg/mock/mockhbstream.HeartbeatStream')
 def stream(channel):
  a=alloc('github.com/tikv/pd/pkg/mock/mockhbstream.HeartbeatStream');cmd(f'set {a}.ch = {channel}');iface=alloc('github.com/tikv/pd/pkg/schedule/hbstream.HeartbeatStream')
  addr=re.search(r'\((0x[0-9a-f]+)\)$',iface)[1];data=re.search(r'\((0x[0-9a-f]+)\)$',a)[1]
  cmd(f'set (*[2]uintptr)({addr})[0] = 0x1f192e0');cmd(f'set (*[2]uintptr)({addr})[1] = (*[1]uintptr)({data})[0]');return iface
 good=stream(f'{q}.msgCh');bad=stream('nil')
 peer=alloc('github.com/pingcap/kvproto/pkg/metapb.Peer');cmd(f'set {peer}.Id = 1');cmd(f'set {peer}.StoreId = 1')
 region=alloc('github.com/tikv/pd/pkg/core.RegionInfo');meta=alloc('github.com/pingcap/kvproto/pkg/metapb.Region');cmd(f'set {meta}.Id = 55');cmd(f'set {region}.meta = {meta}');cmd(f'set {region}.leader = {peer}');op=alloc('github.com/tikv/pd/pkg/schedule/hbstream.Operation')
 def send():cmd(f'call -unsafe {hs}.SendMsg({region},{op})');wait()
 def bind(i,st):cmd(f'call -unsafe {hs}.BindStream({i},*{st})');wait()
 send();observe('push_skip');bind(1,good);send();observe('push_ok')
 bind(1,bad);send();[wait() for _ in range(110)];observe('push_err')
 bind(1,good);bind(2,bad);bind(99,good);cmd('set cluster.BasicCluster.StoresInfo.stores[3].meta.NodeState = 3');bind(3,good)
 for cycle in range(100):
  [wait() for _ in range(100)]
  n=int(re.search(r'~r0: (\d+)',cmd(f'call {q}.MsgLength()'))[1])
  if n>1:break
 else:raise AssertionError('keepalive not observed')
 [wait() for _ in range(110)];observe('keepalive')
 raw=cmd(f'call -unsafe "encoding/json".Marshal({q}.msgCh.buf[1])');nums=re.search(r'\[([0-9, ]+)\]',raw)[1];keepalive=json.loads(bytes(int(n) for n in nums.split(',')))
 cmd(f'call -unsafe {hs}.Close()');cmd(f'call -unsafe {hs}.Close()');observe('closed_twice')
 # Saturate a real bounded queue without a consumer; Close and subsequent enqueue must return.
 sat=newhs(False)
 for i in range(1024):cmd(f'call -unsafe {sat}.SendMsg({region},{op})')
 before=int(re.search(r'~r0: (\d+)',cmd(f'call {sat}.MsgLength()'))[1]);assert before==1024
 cmd(f'call -unsafe {sat}.Close()');cmd(f'call -unsafe {sat}.Close()');cmd(f'call -unsafe {sat}.SendMsg({region},{op})')
 after=int(re.search(r'~r0: (\d+)',cmd(f'call {sat}.MsgLength()'))[1]);assert after==1024
 desc=cmd('print hbstream.heartbeatStreamCounter.MetricVec.metricMap.desc')
 cmd('exit');assert p.wait()==0
 Path('docs/rust/schedule-execution/fixtures/task_014.json').write_text(json.dumps(dict(source_commit='836fdc7cf3ef34f7eabc9d051418995bb97efabe',cases=cases,keepalive=keepalive,saturated_queue=dict(before=before,after=after,close_calls=2,send_after_close_returned=True),descriptor=desc),indent=2)+'\n')
finally:
 if p.poll() is None:p.terminate();p.wait()
 gzip.open(out/'oracle.json.gz','wt').write(json.dumps(dict(argv=argv,exit_code=p.returncode,exchanges=exchanges),indent=2)+'\n')
 gzip.open(out/'oracle.commands.gz','wt').write('\n'.join(commands)+'\n')
