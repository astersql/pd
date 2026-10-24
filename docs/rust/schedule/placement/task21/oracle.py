"""Read-only existing PD executable; isolated HTTP/gRPC placement oracle."""
import base64, hashlib, json, pathlib, subprocess, time, urllib.request
root=pathlib.Path(__file__).parent
name='pd-task21-oracle'
args=['--name','placement21','--data-dir','/tmp/placement21','--client-urls','http://0.0.0.0:2379','--advertise-client-urls','http://127.0.0.1:2379','--peer-urls','http://127.0.0.1:2380','--log-file','/tmp/placement21.log']
subprocess.run(['docker','run','-d','--name',name,'-p','127.0.0.1::2379','-v','/tmp/pd-types-oracle:/oracle:ro','alpine:edge','/oracle/pd-classic-linux',*args],check=True)
try:
 g={'__file__':str(root.parents[1]/'labeler/task14/oracle.py')}
 exec((root.parents[1]/'labeler/task14/oracle.py').read_text().split('cases=[]')[0].replace('pd-task14-oracle',name),g)
 http=g['http']; cluster=g['cluster']
 def req(path,value=None,method=None):
  request=urllib.request.Request(g['base']+path,data=None if value is None else json.dumps(value).encode(),headers={'Content-Type':'application/json'},method=method)
  try:
   with urllib.request.urlopen(request,timeout=10) as r:
    body=r.read()
    try: value=json.loads(body)
    except ValueError: value=body.decode()
    return r.status,value
  except __import__('urllib.error',fromlist=['HTTPError']).HTTPError as e:return e.code,json.load(e)
 result={'sha256':hashlib.sha256(pathlib.Path('/tmp/pd-types-oracle/pd-classic-linux').read_bytes()).hexdigest(),'cases':[]}
 base={'group_id':'task21','id':'r','role':'voter','count':1,'start_key':'','end_key':''}
 for case_name,patch in [('minimal',{}),('collections',{'index':-2,'override':True,'location_labels':['zone'],'label_constraints':[{'key':'zone','op':'notIn','values':['a','b']}]}),('role',{'role':'learner'}),('witness',{'is_witness':True})]:
  r=dict(base,**patch)
  status,out=req('/pd/api/v1/config/rule',r,'POST')
  case={'name':case_name,'input':r,'status':status,'output':out}
  if status==200:
   code,value=req('/pd/api/v1/config/rule/task21/r');assert code==200,(code,value)
   case['read']=value
  result['cases'].append(case)
 # Actual duplicate and null object fields through encoding/json before adjustment.
 raw='{"group_id":"task21","id":"r","role":"voter","count":1,"location_labels":["zone"],"location_labels":null}'
 request=urllib.request.Request(g['base']+'/pd/api/v1/config/rule',data=raw.encode(),headers={'Content-Type':'application/json'},method='POST')
 with urllib.request.urlopen(request) as response: assert response.status==200
 code,value=req('/pd/api/v1/config/rule/task21/r');assert code==200
 result['duplicate_null']={'input':raw,'read':value}
 code,bundle=req('/pd/api/v1/config/placement-rule/task21');result['bundle']={'status':code,'output':bundle}
 result['kv']=http('/v3/kv/range',dict(key=base64.b64encode(f'/pd/{cluster}/rules/7461736b3231-72'.encode()).decode()))
 # Run real Go MatchLabelConstraints via region fitting of one voter peer.
 import grpc
 with grpc.insecure_channel('127.0.0.1:'+g['port']) as channel:
  heartbeat=g['blob'](1,g['header'])+g['blob'](2,g['region'])+g['blob'](3,g['integer'](1,3)+g['integer'](2,1))
  call=channel.stream_stream('/pdpb.PD/RegionHeartbeat')(iter([heartbeat]),timeout=2)
  try: next(call)
  except (grpc.RpcError,StopIteration):pass
 code,value=req('/pd/api/v1/config/rule_group',dict(id='task21',index=10,override=True),'POST');assert code==200,(code,value)
 result['group']=req('/pd/api/v1/config/rule_group/task21')[1]
 result['matching']=[]
 for labels in [{},{'zone':'x'},{'zone':'y'}]:
  # PutStore replaces the complete label list; no source files are mounted writable.
  store=g['store']+b''.join(g['blob'](4,g['blob'](1,k.encode())+g['blob'](2,v.encode())) for k,v in labels.items())
  with grpc.insecure_channel('127.0.0.1:'+g['port']) as channel:
   channel.unary_unary('/pdpb.PD/PutStore')(g['blob'](1,g['header'])+g['blob'](2,store),timeout=10)
  for op in ['in','notIn','exists','notExists']:
   dummy_labels={'zone':'x'} if op in ['in','exists'] else {}
   dummy_id=5 if dummy_labels else 4
   dummy=g['integer'](1,dummy_id)+g['blob'](2,('127.0.0.1:'+str(20160+dummy_id)).encode())+b''.join(g['blob'](4,g['blob'](1,k.encode())+g['blob'](2,v.encode())) for k,v in dummy_labels.items())
   with grpc.insecure_channel('127.0.0.1:'+g['port']) as channel:
    print('dummy',op,channel.unary_unary('/pdpb.PD/PutStore')(g['blob'](1,g['header'])+g['blob'](2,dummy),timeout=10).hex(),flush=True)
   constraint=dict(key='zone',op=op)
   if op in ['in','notIn']:constraint['values']=['x']
   r=dict(base,label_constraints=[constraint])
   code,value=req('/pd/api/v1/config/rules',[r],'POST');assert code==200,(labels,op,code,value,req('/pd/api/v1/stores'))
   code,fit=req('/pd/api/v1/config/rules/region/2/detail');assert code==200,(code,fit)
   assert len(fit['rule-fits'])==1,fit
   peers=fit['rule-fits'][0]['peers'] or []
   result['matching'].append(dict(labels=labels,constraint=constraint,matched=bool(peers),fit=fit))
 (root/'go.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
finally:subprocess.run(['docker','rm','-f',name],check=True)
