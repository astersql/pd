"""Read-only existing PD executable; real HTTP + embedded etcd WAL restart."""
import base64, hashlib, json, pathlib, subprocess, time, urllib.request
root=pathlib.Path(__file__).parent
name='pd-task19-oracle'
args=['--name','query19','--data-dir','/tmp/query19','--client-urls','http://0.0.0.0:2379','--advertise-client-urls','http://127.0.0.1:2379','--peer-urls','http://127.0.0.1:2380','--log-file','/tmp/query19.log']
subprocess.run(['docker','run','-d','--name',name,'-p','127.0.0.1::2379','-v','/tmp/pd-types-oracle:/oracle:ro','alpine:edge','/oracle/pd-classic-linux',*args],check=True)
try:
 g={'__file__':str(root.parent/'task14/oracle.py')}
 exec((root.parent/'task14/oracle.py').read_text().split('cases=[]')[0].replace('pd-task14-oracle',name),g)
 http=g['http']; cluster=g['cluster']; prefix=f'/pd/{cluster}/region_label/'
 def req(path,value=None,method=None):
  request=urllib.request.Request(g['base']+path,data=None if value is None else json.dumps(value).encode(),headers={'Content-Type':'application/json'},method=method)
  try:
   with urllib.request.urlopen(request,timeout=10) as r:
    body=r.read()
    try: value=json.loads(body)
    except ValueError: value=body.decode()
    return r.status,value
  except __import__('urllib.error',fromlist=['HTTPError']).HTTPError as e:return e.code,json.load(e)
 def rule(id,start='10',end='20'):
  return dict(id=id,index=0,labels=[dict(key='k',value=id)],rule_type='key-range',data=[dict(start_key=start,end_key=end)])
 def put(key,value):
  return http('/v3/kv/put',dict(key=base64.b64encode(key.encode()).decode(),value=base64.b64encode(value.encode()).decode()))
 result={'sha256':hashlib.sha256(pathlib.Path('/tmp/pd-types-oracle/pd-classic-linux').read_bytes()).hexdigest()}
 def patch(value):return req('/pd/api/v1/config/region-label/rules',value,'PATCH')
 result['seed']=http('/pd/api/v1/config/region-label/rule',dict(id='query19',index=5,labels=[dict(key='k',value='high'),dict(key='schedule',value='DeNy')],rule_type='key-range',data=[dict(start_key='',end_key='')]))
 result['crossing']=http('/pd/api/v1/region/id/2/labels')
 req('/pd/api/v1/config/region-label/rule/keyspaces%2F0',method='DELETE')
 result['single']=http('/pd/api/v1/region/id/2/label/k')
 result['labels']=http('/pd/api/v1/region/id/2/labels')
 result['all']=http('/pd/api/v1/config/region-label/rules')
 assert result['single']==(200,'high'),result
 print(json.dumps(result,indent=2))
 (root/'go.json').write_text(json.dumps(result,indent=2)+'\n')
finally:subprocess.run(['docker','rm','-f',name],check=True)
