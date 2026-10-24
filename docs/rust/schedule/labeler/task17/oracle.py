"""Read-only existing PD executable; real HTTP + embedded etcd WAL restart."""
import base64, hashlib, json, pathlib, subprocess, time, urllib.request
root=pathlib.Path(__file__).parent
name='pd-task17-oracle'
args=['--name','restore17','--data-dir','/tmp/restore17','--client-urls','http://0.0.0.0:2379','--advertise-client-urls','http://127.0.0.1:2379','--peer-urls','http://127.0.0.1:2380','--log-file','/tmp/restore17.log']
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
 result['set']=http('/pd/api/v1/config/region-label/rule',rule('restore'))
 result['replace']=http('/pd/api/v1/config/region-label/rule',rule('restore','30','40'))
 result['delete_missing']=req('/pd/api/v1/config/region-label/rule/absent',method='DELETE')
 result['delete']=req('/pd/api/v1/config/region-label/rule/restore',method='DELETE')
 result['set_again']=http('/pd/api/v1/config/region-label/rule',rule('restore'))
 seeded={'corrupt':'{','invalid':json.dumps(rule('invalid')|{'labels':[]}), 'alias':json.dumps(rule('embedded','50','60'))}
 for k,v in seeded.items():assert put(prefix+k,v)[0]==200
 result['seeded']=seeded
 subprocess.run(['docker','restart',name],check=True)
 port=subprocess.check_output(['docker','port',name,'2379/tcp'],text=True).strip().rsplit(':',1)[1];g['base']='http://127.0.0.1:'+port
 for _ in range(160):
  try:
   status,rules=http('/pd/api/v1/config/region-label/rules')
   if status==200 and any(r['id']=='embedded' for r in rules):break
  except Exception:pass
  time.sleep(.25)
 else:raise RuntimeError('restart did not restore')
 result['restored']=sorted([r for r in rules if r['id'] in ['restore','embedded']],key=lambda r:r['id'])
 out=http('/v3/kv/range',dict(key=base64.b64encode(prefix.encode()).decode(),range_end=base64.b64encode((prefix[:-1]+'0').encode()).decode()))
 assert out[0]==200,out
 result['persisted']=sorted(base64.b64decode(k['key']).decode().removeprefix(prefix) for k in out[1].get('kvs',[]))
 assert 'corrupt' not in result['persisted'] and 'invalid' not in result['persisted']
 assert 'alias' in result['persisted'] and len(result['restored'])==2
 (root/'go.json').write_text(json.dumps(result,indent=2)+'\n'); print(json.dumps(result,indent=2))
finally:subprocess.run(['docker','rm','-f',name],check=True)
