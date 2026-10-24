"""Existing binary only; disposable container, no repository mount or Go build."""
import base64, concurrent.futures, hashlib, json, pathlib, subprocess, time
root = pathlib.Path(__file__).parent
container = 'pd-task13-oracle'
def request(path, value=None, allow_error=False):
    args=['docker','exec',container,'wget','-qO-']
    if value is not None: args += ['--header=Content-Type: application/json','--post-data='+json.dumps(value)]
    p=subprocess.run(args+['http://127.0.0.1:2379'+path],text=True,capture_output=True)
    if allow_error: return {'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr}
    p.check_returncode()
    return json.loads(p.stdout)
for _ in range(60):
    try:
        before=request('/pd/api/v1/config'); break
    except Exception: time.sleep(.5)
else: raise RuntimeError('readiness timeout')
# Disjoint concurrent partial updates must survive in both active and stored JSON.
patches=[{'max-snapshot-count':99},{'max-pending-peer-count':88},{'leader-schedule-limit':7},{'region-schedule-limit':13}]
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
    list(pool.map(lambda p: request('/pd/api/v1/config/schedule',p),patches))
active=request('/pd/api/v1/config')['schedule']
cluster=request('/pd/api/v1/cluster')['id']
key=base64.b64encode(f'/pd/{cluster}/config'.encode()).decode()
def stored():
    return json.loads(base64.b64decode(request('/v3/kv/range',{'key':key})['kvs'][0]['value']))['schedule']
persisted=stored()
expected={k:v for p in patches for k,v in p.items()}
for k,v in expected.items(): assert active[k]==persisted[k]==v,(k,active[k],persisted[k])
alarm=request('/v3/maintenance/alarm',{'action':'ACTIVATE','memberID':'0','alarm':'NOSPACE'})
failed=request('/pd/api/v1/config/schedule',{'max-snapshot-count':100},True)
assert failed['exit']!=0, failed
assert request('/pd/api/v1/config')['schedule']==active
assert stored()==persisted
request('/v3/maintenance/alarm',{'action':'DEACTIVATE','memberID':'0','alarm':'NOSPACE'})
request('/pd/api/v1/config/schedule',{'max-snapshot-count':101})
assert request('/pd/api/v1/config')['schedule']['max-snapshot-count']==stored()['max-snapshot-count']==101
out={'sha256':hashlib.sha256(pathlib.Path('/tmp/pd-types-oracle/pd-classic-linux').read_bytes()).hexdigest(),'concurrent_active_and_stored':expected,'alarm':alarm,'failed_update':failed,'rollback_active_equal':True,'rollback_stored_equal':True,'subsequent_update':101}
(root/'go.out').write_text(json.dumps(out,indent=2)+'\n')
print(json.dumps(out))
