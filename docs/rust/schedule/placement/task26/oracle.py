"""Read-only existing PD binary; hand-encoded pinned protobuf transport only."""
import pathlib, subprocess, json, hashlib, time, urllib.request
import grpc
root=pathlib.Path(__file__).parent
name='pd-task26-oracle'
subprocess.run(['docker','run','-d','--name',name,'-p','127.0.0.1::2379','-v','/tmp/pd-types-oracle:/oracle:ro','alpine:edge','/oracle/pd-classic-linux','--name','placement26','--data-dir','/tmp/placement26','--client-urls','http://0.0.0.0:2379','--advertise-client-urls','http://127.0.0.1:2379','--peer-urls','http://127.0.0.1:2380'],check=True,stdout=subprocess.DEVNULL)
try:
 g={'__file__':str(root.parents[1]/'labeler/task14/oracle.py')}
 exec((root.parents[1]/'labeler/task14/oracle.py').read_text().split('cases=[]')[0].replace('pd-task14-oracle',name),g)
 integer,blob,header=g['integer'],g['blob'],g['header']
 def http(path,value=None):return g['http']('/pd/api/v1/'+path,value)
 channel=grpc.insecure_channel(g['base'].split('//')[1])
 assert http('config/replicate',{'max-replicas':3})[0]==200
 for sid in [2,3,4]:
  store=integer(1,sid)+blob(2,('127.0.0.1:'+str(20160+sid)).encode())
  if sid==4:store+=blob(4,blob(1,b'oracle')+blob(2,b'reserve'))
  channel.unary_unary('/pdpb.PD/PutStore')(blob(1,header)+blob(2,store),timeout=10)
 def peer(p):return integer(1,p['id'])+integer(2,p['store_id'])+integer(3,p.get('role',0))+integer(4,int(p.get('is_witness',False)))
 cases=[]
 for role in ['voter','leader','follower','learner']:
  for count in ([1] if role=='leader' else [1,3,5]):cases.append(dict(name=role+str(count),role=role,count=count))
 cases += [dict(name='witness_on',role='voter',count=1,witness=True,support=True),dict(name='witness_off',role='voter',count=1,witness=True,support=False),dict(name='missing_store',role='voter',count=4,missing=True),dict(name='health_order',role='voter',count=1,health=True),dict(name='health_fallback',role='voter',count=3,health=True),dict(name='joint_voters',role='voter',count=3,joint=True)]
 out={'sha256':hashlib.sha256(pathlib.Path('/tmp/pd-types-oracle/pd-classic-linux').read_bytes()).hexdigest(),'cases':[]}
 for seq,c in enumerate(cases,10):
  peers=[dict(id=1,store_id=1),dict(id=2,store_id=2,role=1),dict(id=3,store_id=3,is_witness=True)]
  if c.get('joint'):peers[1]['role']=2;peers[2]['role']=3
  if c.get('health'):peers[1]['role']=0;peers[2]['is_witness']=False
  if c.get('missing'):peers.append(dict(id=4,store_id=999))
  support=c.get('support',False)
  assert http('config/schedule',{'enable-witness':str(support).lower()})[0]==200
  rule=dict(group_id='pd',id='target',index=-1,role=c['role'],count=c['count'],is_witness=c.get('witness',False))
  # Validation caps witness count at max-replicas/2; one witness fits.
  status,body=http('config/rules',[rule,dict(group_id='pd',id='default',role='voter',count=3,label_constraints=[dict(key='oracle',op='in',values=['reserve'])])])
  assert status==200,(c,status,body)
  region=integer(1,2)+blob(4,integer(1,seq)+integer(2,seq))+b''.join(blob(5,peer(p)) for p in peers)
  heartbeat=blob(1,header)+blob(2,region)+blob(3,peer(peers[0]))+integer(10,1)
  if c.get('health'):heartbeat+=blob(4,blob(1,peer(peers[0]))+integer(2,10))+blob(5,peer(peers[1]))
  stream=channel.stream_stream('/pdpb.PD/RegionHeartbeat')(iter([heartbeat]),timeout=3)
  for _ in range(60):
   status,r=http('region/id/2')
   if status==200 and r.get('epoch',{}).get('version')==seq:break
   time.sleep(.05)
  else:raise RuntimeError(('heartbeat not applied',c,r))
  status,result=http('config/rules/region/2/detail');assert status==200,(status,result)
  stream.cancel()
  c.update(peers=peers,leader=1,output=result,stores=[http('store/'+str(sid))[1] for sid in [1,2,3,4]])
  out['cases'].append(c)
  print(c['name'],json.dumps(result),flush=True)
 channel.close()
 (root/'go.json').write_text(json.dumps(out,indent=2)+'\n')
finally:subprocess.run(['docker','rm','-f',name],check=True,stdout=subprocess.DEVNULL)
