"""Existing Go PD binary, read-only mount, isolated embedded etcd; no Go writes."""
import hashlib,json,pathlib,subprocess,urllib.request,urllib.error
root=pathlib.Path(__file__).parent
name='pd-task25-oracle'
subprocess.run(['docker','run','-d','--name',name,'-p','127.0.0.1::2379','-v','/tmp/pd-types-oracle:/oracle:ro','alpine:edge','/oracle/pd-classic-linux','--name','placement25','--data-dir','/tmp/placement25','--client-urls','http://0.0.0.0:2379','--advertise-client-urls','http://127.0.0.1:2379','--peer-urls','http://127.0.0.1:2380','--log-file','/tmp/placement25.log'],check=True)
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

 def rule(id):return dict(group_id='g',id=id,role='voter',count=1,start_key='',end_key='')
 def record(name):
  out['steps'].append(dict(name=name,rules=request('GET','rules'),groups=request('GET','rule_groups')))
 request('POST','rules',[rule('pre1'),rule('pre2')]);record('set_rules')
 request('POST','rules/batch',[dict(rule('pre3'),action='add'),dict(rule('pre'),action='del',delete_by_id_prefix=True)]);record('prefix_original')
 request('POST','rule_group',dict(id='g',index=8));record('group')
 request('DELETE','rule_group/g');record('reset_group')
 request('POST','placement-rule/g',dict(group_id='g',group_index=9,rules=[dict(id='bundle',role='voter',count=2)]));record('bundle')
 request('POST','placement-rule?partial',[dict(group_id='h',rules=[dict(id='h1',role='voter',count=1)])]);record('merge_bundles')
 request('DELETE','placement-rule/g?regexp');record('regex_delete')
 try:request('POST','rules',[dict(rule('bad'),count=0)])
 except urllib.error.HTTPError as e:out['invalid_error']=dict(status=e.code,body=e.read().decode())
 record('invalid_unchanged')
 request('POST','rules',[dict(rule('keep'),group_id='١'),dict(rule('delete'),group_id='1')])
 request('DELETE','placement-rule/%5E%5Cd%24?regexp');record('ascii_regexp')
 (root/'go.json').write_text(json.dumps(out,indent=2)+'\n')
 print(json.dumps(out,indent=2))
finally:subprocess.run(['docker','rm','-f',name],check=True)
