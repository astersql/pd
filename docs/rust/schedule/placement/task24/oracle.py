"""Existing Go PD binary, read-only mount, isolated embedded etcd; no Go writes."""
import hashlib,json,pathlib,subprocess,urllib.request,urllib.error
root=pathlib.Path(__file__).parent
name='pd-task24-oracle'
subprocess.run(['docker','run','-d','--name',name,'-p','127.0.0.1::2379','-v','/tmp/pd-types-oracle:/oracle:ro','alpine:edge','/oracle/pd-classic-linux','--name','placement24','--data-dir','/tmp/placement24','--client-urls','http://0.0.0.0:2379','--advertise-client-urls','http://127.0.0.1:2379','--peer-urls','http://127.0.0.1:2380','--log-file','/tmp/placement24.log'],check=True)
try:
 import time
 for _ in range(80):
  if subprocess.run(['docker','port',name,'2379/tcp'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode==0:break
  time.sleep(.1)
 g={'__file__':str(root.parents[1]/'labeler/task14/oracle.py')}
 exec((root.parents[1]/'labeler/task14/oracle.py').read_text().split('cases=[]')[0].replace('pd-task14-oracle',name),g)
 out={'sha256':hashlib.sha256(pathlib.Path('/tmp/pd-types-oracle/pd-classic-linux').read_bytes()).hexdigest(),'steps':[]}
 def request(method,path,value=None):
  q=urllib.request.Request(g['base']+'/pd/api/v1/config/'+path,data=None if value is None else json.dumps(value).encode(),headers={'Content-Type':'application/json'},method=method)
  with urllib.request.urlopen(q,timeout=10) as r:
   body=r.read()
   return json.loads(body) if body else None
 r=dict(group_id='g',id='r',role='voter',count=1,start_key='',end_key='')
 def record(name):
  rules=request('GET','rules/group/g')
  groups=request('GET','rule_groups')
  out['steps'].append(dict(name=name,rules=rules,groups=groups))
 record('default');request('POST','rule',r);record('insert')
 original=request('GET','rule/g/r')
 request('POST','rule',original);record('same')
 original['count']=2;request('POST','rule',original);record('update')
 # Inject legacy/corrupt raw etcd records through the existing v3 gRPC API.
 import grpc
 def raw_put(key,value):
  with grpc.insecure_channel(g['base'].removeprefix('http://')) as channel:
   channel.unary_unary('/etcdserverpb.KV/Put')(g['blob'](1,key)+g['blob'](2,value),timeout=10)
 prefix=('/pd/'+str(g['cluster'])+'/rules/').encode()
 raw_put(prefix+b'legacy-key',json.dumps(dict(r,id='legacy')).encode())
 raw_put(prefix+b'bad-json',b'{')
 raw_put(prefix+b'bad-rule',b'{}')
 raw_put(prefix+b'\xff',b'bad json')
 # Restart the actual Go process against its own embedded etcd WAL.
 subprocess.run(['docker','restart',name],check=True)
 g['base']='http://127.0.0.1:'+subprocess.check_output(['docker','port',name,'2379/tcp'],text=True).strip().rsplit(':',1)[1]
 import time
 for _ in range(120):
  try:
   request('GET','rules');break
  except Exception:time.sleep(.25)
 else:raise RuntimeError('restart unavailable')
 record('restart')
 out['restored_rules']=request('GET','rules')
 # Keep only keys in the range response for a stable raw persistence check.
 with grpc.insecure_channel(g['base'].removeprefix('http://')) as channel:
  raw=channel.unary_unary('/etcdserverpb.KV/Range')(g['blob'](1,prefix)+g['blob'](2,prefix[:-1]+b'0'),timeout=10)
 out['raw_range_hex']=raw.hex()
 request('POST','rule_group',dict(id='g',index=8));record('group')
 request('DELETE','rule_group/g');record('default_group')
 request('DELETE','rule/g/r');record('delete')
 (root/'go.json').write_text(json.dumps(out,indent=2)+'\n')
 print(json.dumps(out,indent=2))
finally:subprocess.run(['docker','rm','-f',name],check=True)
