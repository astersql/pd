"""Read-only existing PD binary; hand-encoded pinned protobuf transport only."""
import pathlib, subprocess, json, hashlib, time, urllib.request
import grpc
root=pathlib.Path(__file__).parent
name='pd-task27-oracle'
subprocess.run(['docker','run','-d','--name',name,'-p','127.0.0.1::2379','-v','/tmp/pd-types-oracle:/oracle:ro','alpine:edge','/oracle/pd-classic-linux','--name','placement27','--data-dir','/tmp/placement27','--client-urls','http://0.0.0.0:2379','--advertise-client-urls','http://127.0.0.1:2379','--peer-urls','http://127.0.0.1:2380'],check=True,stdout=subprocess.DEVNULL)
try:
 g={'__file__':str(root.parents[1]/'labeler/task14/oracle.py')}
 exec((root.parents[1]/'labeler/task14/oracle.py').read_text().split('cases=[]')[0].replace('pd-task14-oracle',name),g)
 integer,blob,header=g['integer'],g['blob'],g['header']
 def http(path,value=None):return g['http']('/pd/api/v1/'+path,value)
 channel=grpc.insecure_channel(g['base'].split('//')[1])
 assert http('config/replicate',{'max-replicas':3})[0]==200
 for sid in [1,2,3,4]:
  store=integer(1,sid)+blob(2,('127.0.0.1:'+str(20160+sid)).encode())
  store+=blob(4,blob(1,b'zone')+blob(2,b'a' if sid==1 else b'b'))
  if sid==4:store+=blob(4,blob(1,b'oracle')+blob(2,b'reserve'))
  channel.unary_unary('/pdpb.PD/PutStore')(blob(1,header)+blob(2,store),timeout=10)
 def peer(p):return integer(1,p['id'])+integer(2,p['store_id'])+integer(3,p.get('role',0))+integer(4,int(p.get('is_witness',False)))
 def rule(role='voter',count=1,**kw):return dict(role=role,count=count,**kw)
 cases=[
  dict(name='reserve_leader',rules=[rule(),rule('leader')]),
  dict(name='reserve_follower',rules=[rule(),rule('follower')]),
  dict(name='priority_leader_first',rules=[rule('leader'),rule()]),
  dict(name='orphan_order',rules=[rule()]),
  dict(name='shortage',rules=[rule(count=3),rule('learner',2)]),
  dict(name='role_conversion',rules=[rule('learner'),rule(count=2)]),
  dict(name='health_order',rules=[rule(),rule()],health=True),
  dict(name='witness_on',rules=[rule(),rule(is_witness=True)],support=True),
  dict(name='witness_off',rules=[rule(),rule(is_witness=True)]),
  dict(name='missing_store',rules=[rule(count=4)],missing=True),
  dict(name='later_isolation',rules=[rule(),rule(count=2,location_labels=['zone'])]),
  dict(name='first_isolation',rules=[rule(count=2,location_labels=['zone']),rule()]),
  dict(name='isolation_priority',rules=[rule(count=2,location_labels=['zone']),rule(label_constraints=[dict(key='zone',op='in',values=['a'])])]),
  dict(name='constraint_priority',rules=[rule(label_constraints=[dict(key='zone',op='in',values=['a'])]),rule(count=2,location_labels=['zone'])]),
  dict(name='constraint_backtrack',rules=[rule(),rule(label_constraints=[dict(key='zone',op='in',values=['a'])])]),
 ]
 out={'sha256':hashlib.sha256(pathlib.Path('/tmp/pd-types-oracle/pd-classic-linux').read_bytes()).hexdigest(),'cases':[]}
 for seq,c in enumerate(cases,10):
  peers=[dict(id=1,store_id=1),dict(id=2,store_id=2,role=1),dict(id=3,store_id=3,is_witness=True)]
  if c['name'] not in ['role_conversion','witness_on','witness_off']:
   peers[1]['role']=0;peers[2]['is_witness']=False
  if c.get('joint'):peers[1]['role']=2;peers[2]['role']=3
  if c.get('health'):peers[1]['role']=0;peers[2]['is_witness']=False
  if c.get('missing'):peers.append(dict(id=4,store_id=999))
  support=c.get('support',False)
  assert http('config/schedule',{'enable-witness':str(support).lower()})[0]==200
  rules=[dict(group_id='pd',id='target'+str(i),index=-100+i,**r) for i,r in enumerate(c['rules'])]
  rules.append(dict(group_id='pd',id='default',role='voter',count=3,label_constraints=[dict(key='oracle',op='in',values=['reserve'])]))
  status,body=http('config/placement-rule/pd',dict(group_id='pd',rules=rules))
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
  # Region heartbeat updates witness statistics asynchronously. Capture a
  # stable store snapshot bracketing the actual fit, never rewrite its scores.
  for _ in range(60):
   before=[http('store/'+str(sid))[1] for sid in [1,2,3,4]]
   status,result=http('config/rules/region/2/detail');assert status==200,(status,result)
   after=[http('store/'+str(sid))[1] for sid in [1,2,3,4]]
   counts=lambda entries:{e['store']['id']:e.get('status',{}).get('witness_count',0) for e in entries}
   load=counts(after)
   if counts(before)==load and all(f['witness-score']==(sum(load[p['store_id']] for p in f['peers'] or []) if support and f['rule']['is_witness'] else 0) for f in result['rule-fits']):break
   time.sleep(.05)
  else:raise RuntimeError(('unstable witness snapshot',c,result,after))
  assert [f['rule']['id'] for f in result['rule-fits']]==[r['id'] for r in rules],result
  stream.cancel()
  c.update(peers=peers,leader=1,output=result,stores=after)
  out['cases'].append(c)
  print(c['name'],json.dumps(result),flush=True)
 channel.close()
 (root/'go.json').write_text(json.dumps(out,indent=2)+'\n')
finally:subprocess.run(['docker','rm','-f',name],check=True,stdout=subprocess.DEVNULL)
