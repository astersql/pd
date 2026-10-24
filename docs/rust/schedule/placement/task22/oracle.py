"""Existing Go PD binary, read-only mount, isolated embedded etcd; no Go writes."""
import hashlib,json,pathlib,subprocess,urllib.request,urllib.error
root=pathlib.Path(__file__).parent
name='pd-task22-oracle'
subprocess.run(['docker','run','-d','--name',name,'-p','127.0.0.1::2379','-v','/tmp/pd-types-oracle:/oracle:ro','alpine:edge','/oracle/pd-classic-linux','--name','placement22','--data-dir','/tmp/placement22','--client-urls','http://0.0.0.0:2379','--advertise-client-urls','http://127.0.0.1:2379','--peer-urls','http://127.0.0.1:2380','--log-file','/tmp/placement22.log'],check=True)
try:
 g={'__file__':str(root.parents[1]/'labeler/task14/oracle.py')}
 exec((root.parents[1]/'labeler/task14/oracle.py').read_text().split('cases=[]')[0].replace('pd-task14-oracle',name),g)
 def req(path,value=None):
  q=urllib.request.Request(g['base']+'/pd/api/v1/config/'+path,data=None if value is None else json.dumps(value).encode(),headers={'Content-Type':'application/json'})
  try:
   with urllib.request.urlopen(q,timeout=10) as r:return r.status,json.load(r)
  except urllib.error.HTTPError as e:return e.code,json.load(e)
 q=urllib.request.Request(g['base']+'/pd/api/v1/config',data=json.dumps({'pd-server.key-type':'raw'}).encode(),headers={'Content-Type':'application/json'})
 with urllib.request.urlopen(q) as response: print('raw config',response.status,response.read())
 def rule(id,**kw):return dict(id=id,role='voter',count=1,start_key='',end_key='',**kw)
 def bundle(id,rules,index=0,override=False):return dict(group_id=id,group_index=index,group_override=override,rules=rules)
 cases=[('order',[bundle('g2',[rule('b'),rule('a',index=-1)],2),bundle('g1',[rule('z')]),bundle('g3',[rule('c')],1)]),('override',[bundle('g1',[rule('a')]),bundle('g2',[rule('b'),rule('c',override=True),rule('d',index=1)],1,True),bundle('g3',[rule('e')],2)]),('ranges',[bundle('pd',[rule('default'),dict(rule('meta',index=1,override=True),start_key='a1',end_key='a2')])]),('empty',[]),('gap',[bundle('g',[dict(rule('r'),end_key='a1'),dict(rule('s'),start_key='a2')])]),('prefix',[bundle('g',[dict(rule('r'),start_key='a1')])]),('tail',[bundle('g',[dict(rule('r'),end_key='a2')])]),('leaders',[bundle('g',[dict(rule('a'),role='leader'),dict(rule('b'),role='leader')])]),('no_voter',[bundle('g',[dict(rule('a'),role='follower'),dict(rule('b'),role='learner')])])]
 out={'sha256':hashlib.sha256(pathlib.Path('/tmp/pd-types-oracle/pd-classic-linux').read_bytes()).hexdigest(),'cases':[]}
 epoch=1
 for label,bundles in cases:
  status,value=req('placement-rule',bundles)
  case=dict(name=label,input=bundles,status=status,output=value)
  if status==200:
   case['keys']={k:req('rules/key/'+k)[1] for k in ['00','a1','a2','ff']}
   import grpc
   case['regions']=[]
   for start,end in [(b'',b''),(b'',b'\xa1'),(b'\xa1',b'\xa2'),(b'\xa2',b'')]:
    epoch+=1
    region=g['integer'](1,2)+g['blob'](2,start)+g['blob'](3,end)+g['blob'](4,g['integer'](1,1)+g['integer'](2,epoch))+g['blob'](5,g['integer'](1,3)+g['integer'](2,1))
    heartbeat=g['blob'](1,g['header'])+g['blob'](2,region)+g['blob'](3,g['integer'](1,3)+g['integer'](2,1))
    with grpc.insecure_channel('127.0.0.1:'+g['port']) as channel:
     call=channel.stream_stream('/pdpb.PD/RegionHeartbeat')(iter([heartbeat]),timeout=1)
     try:next(call)
     except (grpc.RpcError,StopIteration):pass
    code,value=req('rules/region/2');assert code==200,(code,value)
    case['regions'].append(dict(start=start.hex(),end=end.hex(),rules=value))
  out['cases'].append(case)
 (root/'go.json').write_text(json.dumps(out,indent=2)+'\n')
 print(json.dumps(out,indent=2))
finally:subprocess.run(['docker','rm','-f',name],check=True)
