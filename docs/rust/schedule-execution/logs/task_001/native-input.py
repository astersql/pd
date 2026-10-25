import base64, hashlib, json, pathlib, shutil, signal, socket, struct, subprocess, time, urllib.request, urllib.error

p = pathlib.Path(__file__).parent
root = pathlib.Path('/Users/Shared/work/dir/data/codes/pd')
events = []
def emit(kind, **values):
    e = dict(kind=kind, elapsed=round(time.monotonic()-began,3), **values)
    events.append(e)
    (p/'native-events.json').write_text(json.dumps(events,indent=2))
    print(json.dumps(e),flush=True)
def port():
    with socket.socket() as s:
        s.bind(('127.0.0.1',0)); return s.getsockname()[1]
def http(path, body=None, method=None):
    payload = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(url+path,data=payload,method=method,headers={'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(req,timeout=8) as r: return r.status,r.read().decode()
    except urllib.error.HTTPError as e: return e.code,e.read().decode()
def varint(n):
    b=bytearray()
    while n>127: b.append((n&127)|128); n>>=7
    b.append(n); return bytes(b)
def vi(tag,n): return varint(tag*8)+varint(n)
def bs(tag,b):
    if isinstance(b,str): b=b.encode()
    return varint(tag*8+2)+varint(len(b))+b
def grpc(method, payload):
    data=b'\0'+struct.pack('>I',len(payload))+payload
    r=subprocess.run(['curl','--http2-prior-knowledge','--silent','--show-error','--max-time','8','--request','POST','--header','content-type: application/grpc','--header','te: trailers','--data-binary','@-',url+'/pdpb.PD/'+method],input=data,capture_output=True)
    emit('grpc',method=method,input_hex=payload.hex(),output_hex=r.stdout.hex(),stderr=r.stderr.decode(),exit=r.returncode)
    assert r.returncode==0
    assert len(r.stdout)>=5, r.stdout
    return r.stdout
def samples():
    status,text=http('/metrics'); assert status==200
    return [x for x in text.splitlines() if x.startswith(('pd_cluster_progress{','pd_cluster_speed{','pd_cluster_eta{')) and 'store="1"' in x and 'action="removing"' in x]
def wait_for(fn, timeout, label):
    start=time.monotonic()
    while time.monotonic()-start<timeout:
        value=fn()
        if value:return value
        assert child.poll() is None,child.returncode
        time.sleep(2)
    raise AssertionError('timeout: '+label)
def interrupted(s,f): raise SystemExit(128+s)
signal.signal(signal.SIGTERM,interrupted);signal.signal(signal.SIGINT,interrupted)
began=time.monotonic()
server=root/'target/rust-slot-2/pd-oracle47'
probe=root/'target/rust-slot-2/debug/examples/region_heartbeat_oracle'
for src,name in [(server,'pd-oracle'),(probe,'region-probe')]:
    shutil.copy2(src,p/name)
    emit('binary',source=str(src),sha256=hashlib.sha256((p/name).read_bytes()).hexdigest())
(p/'config.toml').write_text('quota-backend-bytes = "32MiB"\nleader-lease = 60\n[pd-server]\nuse-region-storage = false\n[schedule]\nenable-heartbeat-concurrent-runner = false\nschedulers = []\n[replication]\nmax-replicas = 1\nenable-placement-rules = false\n')
c,e=port(),port();url=f'http://127.0.0.1:{c}';peer=f'http://127.0.0.1:{e}'
cmd=[str(p/'pd-oracle'),'--name','admission-native','--data-dir',str(p/'data'),'--client-urls',url,'--advertise-client-urls',url,'--peer-urls',peer,'--advertise-peer-urls',peer,'--initial-cluster',f'admission-native={peer}','--config',str(p/'config.toml'),'--log-level','info']
emit('start',argv=cmd,url=url)
with (p/'server.out').open('w') as log:
    child=subprocess.Popen(cmd,stdout=log,stderr=subprocess.STDOUT)
    try:
        def healthy():
            try:
                status,body=http('/pd/api/v1/health'); return status==200 and 'true' in body
            except Exception:return False
        wait_for(healthy,70,'health')
        time.sleep(5)
        r=subprocess.run([str(p/'region-probe')],env={'PD_ORACLE_URL':url},capture_output=True,text=True,timeout=30)
        emit('existing_probe',exit=r.returncode,stdout=r.stdout,stderr=r.stderr);assert r.returncode==0
        status,raw=http('/pd/api/v1/cluster');emit('cluster',status=status,body=raw);assert status==200
        cluster=json.loads(raw)['id']; header=bs(1,vi(1,cluster))
        for store_id in [2,3,4]:
            store=vi(1,store_id)+bs(2,f'127.0.0.1:{29990+store_id}')+bs(5,'8.5.0')+vi(13,1)
            grpc('PutStore',header+bs(2,store))
        status,raw=http('/pd/api/v1/stores');emit('initial_stores',status=status,body=raw)
        time.sleep(12)
        status,raw=http('/pd/api/v1/store/1/state?state=Offline',{},'POST');emit('offline',status=status,body=raw);assert status==200
        def active_progress():
            status,text=http('/pd/api/v1/stores/progress?id=1')
            if status==200 and samples():return {'status':status,'body':text,'samples':samples()}
        emit('removing_progress',**wait_for(active_progress,40,'removing metrics'))
        status,raw=http('/pd/api/v1/store/1/state?state=Up',{},'POST');emit('up',status=status,body=raw);assert status==200
        wait_for(lambda:http('/pd/api/v1/stores/progress?id=1')[0]==404,35,'completed progress')
        first=samples();emit('completed_before_expiry',samples=first);assert len(first)==3
        time.sleep(15)
        retained=samples();emit('completed_retained',samples=retained);assert len(retained)==3
        wait_for(lambda:not samples(),150,'native GC deletion')
        emit('gc_deleted',samples=samples());assert not samples()
        status,raw=http('/v3/maintenance/status',{},'POST');emit('etcd_gateway',status=status,body=raw)
        status,raw=http('/pd/api/v1/schedulers',{'name':'evict-slow-store-scheduler'},'POST');emit('create_scheduler',status=status,body=raw)
        emit('native_gc_success')
    finally:
        child.terminate()
        try:child.wait(timeout=20)
        except subprocess.TimeoutExpired:child.kill();child.wait()
        emit('joined',exit=child.returncode)
        shutil.rmtree(p/'data',ignore_errors=True)
