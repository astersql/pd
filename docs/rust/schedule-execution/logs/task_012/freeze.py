"""Freeze real Go production metrics from existing binaries; never compile Go."""
import subprocess,os,select,time,json,re,hashlib
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_012');tmp=Path('/tmp/pd-sched003');commands=[];transcript=[];exchanges=[]
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
 assert not any(x in s for x in ['could not','has no member','not allowed','can not convert','not found','too many arguments','not enough arguments']),s
 return s

meta='github.com/pingcap/kvproto/pkg/metapb.'
pd='github.com/pingcap/kvproto/pkg/pdpb.'
hb='github.com/tikv/pd/pkg/schedule/hbstream.'
core='github.com/tikv/pd/pkg/core.'
schemas={
 'Peer':{'id':('Id',None),'store_id':('StoreId',None),'role':('Role',None),'is_witness':('IsWitness',None)},
 'RegionEpoch':{'conf_ver':('ConfVer',None),'version':('Version',None)},
 'Region':{'id':('Id',None),'start_key':('StartKey','bytes'),'end_key':('EndKey','bytes'),'region_epoch':('RegionEpoch','RegionEpoch'),'peers':('Peers',['Peer']),'encryption_meta':('EncryptionMeta','EncryptionMeta'),'is_in_flashback':('IsInFlashback',None),'flashback_start_ts':('FlashbackStartTs',None)},
 'EncryptionMeta':{'key_id':('KeyId',None),'iv':('Iv','bytes')},
 'Operation':{'change_peer':('ChangePeer','ChangePeer'),'transfer_leader':('TransferLeader','TransferLeader'),'merge':('Merge','Merge'),'split_region':('SplitRegion','SplitRegion'),'change_peer_v2':('ChangePeerV2','ChangePeerV2'),'switch_witnesses':('SwitchWitnesses','BatchSwitchWitness'),'change_split':('ChangeSplit','ChangeSplit')},
 'ChangePeer':{'peer':('Peer','Peer'),'change_type':('ChangeType',None)},
 'TransferLeader':{'peer':('Peer','Peer'),'peers':('Peers',['Peer'])},
 'Merge':{'target':('Target','Region')},
 'SplitRegion':{'policy':('Policy',None),'keys':('Keys',['bytes'])},
 'ChangePeerV2':{'changes':('Changes',['ChangePeer'])},
 'BatchSwitchWitness':{'switch_witnesses':('SwitchWitnesses',['SwitchWitness'])},
 'SwitchWitness':{'peer_id':('PeerId',None),'is_witness':('IsWitness',None)},
 'ChangeSplit':{'auto_split_enabled':('AutoSplitEnabled',None)}}
def typename(t):
 if t in ['Peer','RegionEpoch','Region']:return meta+t
 if t=='EncryptionMeta':return 'github.com/pingcap/kvproto/pkg/encryptionpb.'+t
 if t=='Operation':return hb+t
 return pd+t
def alloc(t,size=4096):
 a=re.search(r'unsafe.Pointer\((0x[0-9a-f]+)\)',cmd(f'call runtime.mallocgc({size},nil,true)'))[1]
 return f'(*"{t}")({a})'
def value(t,v):
 if v is None:return 'nil'
 if t=='bytes':
  import base64
  b=base64.b64decode(v)
  if not b:return 'nil'
  a=alloc(f'[{len(b)}]uint8')
  for i,n in enumerate(b):cmd(f'set {a}[{i}] = {n}')
  return f'(*{a})[:]'
 if isinstance(t,list):
  if not v:return 'nil'
  elem='[]uint8' if t[0]=='bytes' else '*'+typename(t[0])
  a=alloc(f'[{len(v)}]{elem}')
  for i,x in enumerate(v):cmd(f'set {a}[{i}] = {value(t[0],x)}')
  return f'(*{a})[:]'
 a=alloc(typename(t))
 for k,x in v.items():
  field,sub=schemas[t][k]
  val=json.dumps(x) if sub is None else value(sub,x)
  cmd(f'set {a}.{field} = {val}')
 return a
cases=[]
try:
 read();cmd('break cluster_test.go:300');cmd('continue');cmd('clearall');cmd('config max-variable-recurse 12');cmd('config max-array-values 10000');cmd('config max-string-len 10000')
 # Raw input allocations remain live while invoking production functions.
 cmd('call "runtime/debug".SetGCPercent(-1)')
 peer={'id':18446744073709551615,'store_id':77,'role':3,'is_witness':True}
 epoch={'conf_ver':18446744073709551615,'version':456}
 target={'id':99,'start_key':'AP8=','end_key':'/wA=','region_epoch':epoch,'peers':[peer,{'id':8,'store_id':9,'role':37}], 'encryption_meta':{'key_id':13,'iv':'AP8B'},'is_in_flashback':True,'flashback_start_ts':987}
 ops={
 'change_peer':{'peer':peer,'change_type':2},
 'transfer_leader':{'peer':peer,'peers':[peer,{'id':8,'store_id':9,'role':37}]},
 'merge':{'target':target},
 'split_region':{'policy':2,'keys':['AP8=','','YWJj','AP8=']},
 'change_peer_v2':{'changes':[{'peer':peer,'change_type':1},None,{'peer':None,'change_type':-7}]},
 'switch_witnesses':{'switch_witnesses':[{'peer_id':18446744073709551615,'is_witness':True},None,{'peer_id':4,'is_witness':False}]},
 'change_split':{'auto_split_enabled':True}}
 inputs=[('empty',{}),*[(k,{k:v}) for k,v in ops.items()],('all',ops),('present_empty',{k:{} for k in ops}),('unknown_enums',{'change_peer':{'change_type':-7},'split_region':{'policy':37}}),('split_disabled',{'change_split':{'auto_split_enabled':False}}),('absent_epoch',{}),('zero_epoch',{}),('zero_peer',{}),('nil_entries',{'transfer_leader':{'peers':[None,peer,None]}})]
 for mode in ['classic','scheduling']:
  h=cmd('call -unsafe hbstream.NewTestHeartbeatStreams(cluster.ctx,nil,false)');addr=re.search(r'hbstream.HeartbeatStreams"\)\((0x[0-9a-f]+)\)',h)[1];hs=f'(*hbstream.HeartbeatStreams)({addr})'
  cmd(f'set {hs}.clusterID = 123')
  if mode=='scheduling':cmd(f'call {hs}.typ = "scheduling"')
  for ix,(name,operation) in enumerate(inputs):
   e=None if name=='absent_epoch' else ({} if name=='zero_epoch' else epoch);lp={} if name=='zero_peer' else peer
   m=value('Region',{'id':55,'region_epoch':e});leader=value('Peer',lp);r=alloc(core+'RegionInfo');cmd(f'set {r}.meta = {m}');cmd(f'set {r}.leader = {leader}')
   op=value('Operation',operation);cmd(f'call -unsafe {hs}.SendMsg({r},{op})')
   observed=cmd(f'print {hs}.msgCh.buf[{ix}]')
   assert ('schedulingpb.RegionHeartbeatResponse' if mode=='scheduling' else 'pdpb.RegionHeartbeatResponse') in observed
   raw=cmd(f'call -unsafe "encoding/json".Marshal({hs}.msgCh.buf[{ix}])')
   nums=re.search(r'\[([0-9, ]+)\]',raw)[1];output=json.loads(bytes(int(n) for n in nums.split(',')))
   assert output['region_id']==55
   cmd(f'call -unsafe {hs}.Drain(1)')
   cases.append(dict(id=mode+'_'+name,mode=mode,region_id=55,region_epoch=e,target_peer=lp,operation=operation,output=output));print(cases[-1]['id'],flush=True)
  # No leader must not dereference a nil operation or enqueue anything.
  r=alloc(core+'RegionInfo');cmd(f'call -unsafe {hs}.SendMsg({r},nil)');raw=cmd(f'call {hs}.MsgLength()');assert '~r0: 0' in raw
  cases.append(dict(id=mode+'_no_leader',mode=mode,region_id=0,region_epoch=None,target_peer=None,operation=None,output=None))
  cmd(f'call -unsafe {hs}.Close()')
 cmd('exit');code=p.wait();assert code==0
 Path('docs/rust/schedule-execution/fixtures/task_012.json').write_text(json.dumps(dict(source_commit='836fdc7cf3ef34f7eabc9d051418995bb97efabe',cases=cases),indent=2)+'\n')
finally:
 if p.poll() is None:p.terminate();p.wait()
 (out/'oracle.json').write_text(json.dumps(dict(argv=argv,exit_code=p.returncode,exchanges=exchanges),indent=2)+'\n')
 (out/'oracle.commands').write_text('\n'.join(commands)+'\n')
