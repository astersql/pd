"""Read-only existing Go binary, private container and WAL; no Go source changes."""
import pathlib, subprocess, json, hashlib
root=pathlib.Path(__file__).parent
name='pd-task32-oracle'
try:
 subprocess.run(['docker','run','-d','--name',name,'-p','127.0.0.1::2379','-v','/tmp/pd-types-oracle:/oracle:ro','alpine:edge','/oracle/pd-classic-linux','--name','task32','--data-dir','/tmp/task32-data','--client-urls','http://0.0.0.0:2379','--advertise-client-urls','http://127.0.0.1:2379','--peer-urls','http://127.0.0.1:2380','--log-file','/tmp/task32.log'],check=True)
 g={'__file__':str(root.parents[1]/'labeler/task14/oracle.py')}
 exec((root.parents[1]/'labeler/task14/oracle.py').read_text().split('cases=[]')[0].replace('pd-task14-oracle',name),g)
 http=g['http'];p='/pd/api/v2/affinity-groups'
 request={'affinity_groups':{'range32':{'ranges':[{'start_key':'AA==','end_key':'AQ=='},{'start_key':'AQ==','end_key':'/w=='}]}}}
 status,response=http(p,request);assert status==200,(status,response)
 status,rule=http('/pd/api/v1/config/region-label/rule/affinity_group%2Frange32');assert status==200,(status,rule)
 result={'sha256':hashlib.sha256(pathlib.Path('/tmp/pd-types-oracle/pd-classic-linux').read_bytes()).hexdigest(),'request':request,'label_rule':rule}
 (root/'go.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
finally:
 subprocess.run(['docker','rm','-f',name],check=True)
