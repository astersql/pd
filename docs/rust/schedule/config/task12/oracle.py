"""Run the existing read-only Go binary in an isolated disposable container.
No repository mount; all database/log writes stay under the container's /tmp.
"""
import base64, json, pathlib, subprocess, time
root = pathlib.Path(__file__).parent
container = 'pd-task12-oracle'
args = ['/oracle/pd-classic-linux', '--name', 'task12', '--data-dir', '/tmp/task12-data', '--client-urls', 'http://127.0.0.1:2379', '--peer-urls', 'http://127.0.0.1:2380', '--log-file', '/tmp/task12.log']
def command(*args):
    return subprocess.check_output(['docker','exec',container,*args], text=True)
def request(path, value=None):
    args=['wget','-qO-']
    if value is not None: args+=['--header=Content-Type: application/json','--post-data='+json.dumps(value)]
    return json.loads(command(*args,'http://127.0.0.1:2379'+path))
def ready():
    for _ in range(60):
        try: return request('/pd/api/v1/config')
        except (subprocess.CalledProcessError, json.JSONDecodeError): time.sleep(.5)
    raise RuntimeError('PD readiness timeout')
before=ready()
request('/pd/api/v1/config/schedule', {'max-snapshot-count':10})
updated=ready()
assert updated['schedule']['max-snapshot-count']==10
assert updated['replication']['max-replicas']==3
cluster=request('/pd/api/v1/cluster')['id']
key=base64.b64encode(f'/pd/{cluster}/config'.encode()).decode()
persisted=request('/v3/kv/range',{'key':key})
stored=json.loads(base64.b64decode(persisted['kvs'][0]['value']))
(root/'go-persisted.json').write_text(json.dumps(stored,indent=2,sort_keys=True)+'\n')
command('sh','-c','kill -TERM $(pidof pd-classic-linux)')
for _ in range(60):
    if subprocess.run(['docker','exec',container,'pidof','pd-classic-linux'],stdout=subprocess.DEVNULL).returncode!=0: break
    time.sleep(.5)
else: raise RuntimeError('shutdown timeout')
subprocess.check_call(['docker','exec','-d',container,*args])
after=ready()
for section in ['schedule','replication','pd-server','replication-mode','label-property','keyspace','micro-service','cluster-version']:
    assert after[section]==updated[section],section
summary={'binary_sha256':command('sha256sum','/oracle/pd-classic-linux').split()[0], 'max-snapshot-count':after['schedule']['max-snapshot-count'],'max-replicas':after['replication']['max-replicas'],'max-movable-hot-peer-size':after['schedule']['max-movable-hot-peer-size'],'restart_equal_sections':['schedule','replication','pd-server','replication-mode','label-property','keyspace','micro-service','cluster-version'],'store_present':'store' in stored,'persisted_name':stored['name'],'enable-placement-rules':stored['replication']['enable-placement-rules']}
(root/'go.out').write_text(json.dumps(summary,ensure_ascii=False,indent=2)+'\n')
print(json.dumps(summary,ensure_ascii=False))
