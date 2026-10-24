"""Read-only existing PD executable; real HTTP + embedded etcd WAL restart."""
import base64, hashlib, json, pathlib, subprocess, time, urllib.request
root=pathlib.Path(__file__).parent
name='pd-task18-oracle'
args=['--name','txn18','--data-dir','/tmp/txn18','--client-urls','http://0.0.0.0:2379','--advertise-client-urls','http://127.0.0.1:2379','--peer-urls','http://127.0.0.1:2380','--log-file','/tmp/txn18.log']
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
 result['seed']=http('/pd/api/v1/config/region-label/rule',rule('existing'))
 result['patch']=patch(dict(sets=[rule('r','10','20'),rule('r','30','40'),rule('s','50','60')],deletes=['existing','r','absent']))
 result['rules']=sorted([r for r in http('/pd/api/v1/config/region-label/rules')[1] if r['id'] in ['r','s','existing']],key=lambda r:r['id'])
 result['invalid']=patch(dict(sets=[rule('x'),rule('')],deletes=['r']))
 result['after_invalid']=sorted([r for r in http('/pd/api/v1/config/region-label/rules')[1] if r['id'] in ['r','s','existing','x']],key=lambda r:r['id'])
 print('responses',result,flush=True)
 assert result['patch'][0]==200 and result['invalid'][0]>=400
 assert result['rules']==result['after_invalid']
 (root/'go.json').write_text(json.dumps(result,indent=2)+'\n'); print(json.dumps(result,indent=2))
finally:subprocess.run(['docker','rm','-f',name],check=True)
