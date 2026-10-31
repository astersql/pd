#!/usr/bin/env python3
"""Read-only HTTP service-limit differential using the existing Go executable."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('health', ROOT/'scripts/http-health-oracle.py')
health = importlib.util.module_from_spec(spec)
spec.loader.exec_module(health)


def request(url, method, path, body=''):
    req = urllib.request.Request(url+'/pd/api/v1/'+path, data=body.encode(), method=method)
    try:
        response = urllib.request.urlopen(req, timeout=5)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        body = response.read().decode()
        try:
            body = json.loads(body)
        except ValueError:
            pass
        return [response.status, body]


def observe(url):
    responses = []
    def run(method, path, body='', status=200):
        result = request(url, method, path, body)
        assert result[0] == status, result
        responses.append(result)
    rate = 'service-middleware/config/rate-limit'
    for body in [{}, {'type':'x'}, {'type':'label'}, {'type':'label','label':'missing'},
                 {'type':'path'}, {'type':'path','path':'/pd/api/v1/health'},
                 {'type':'label','label':'SetRateLimitConfig','qps':1},
                 {'type':'label','label':'SetGRPCRateLimitConfig','concurrency':1}]:
        run('POST', rate, json.dumps(body), 400)
    run('POST', rate, json.dumps({'type':'label','label':'GetHealthStatus'}))
    run('POST', rate, json.dumps({'type':'path','path':'/pd/api/v1/health','method':'GET','qps':.0001}))
    assert request(url,'GET','health')[0] == 200
    run('GET','health',status=429)
    run('POST','service-middleware/config','{"enable-rate-limit":"false"}')
    assert request(url,'GET','health')[0] == 200
    run('POST','service-middleware/config','{"rate-limit.enable-rate-limit":"true"}')
    run('GET','health',status=429)
    run('POST',rate,'{"type":"label","label":"GetHealthStatus","qps":0}')
    assert request(url,'GET','health')[0] == 200
    run('POST',rate,'{"type":"label","label":"SetServiceMiddlewareConfig","concurrency":1}')
    run('POST','service-middleware/config','{"enable-audit":"false"}')
    run('POST','service-middleware/config','{}')
    return responses


def main():
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, health.contract.interrupted)
    target = Path(os.environ['CARGO_TARGET_DIR'])
    assert target in [ROOT/f'target/rust-slot-{n}' for n in range(1,11)]
    assert (ROOT/f'target/rust-slot-locks/slot-{target.name.rsplit("-",1)[1]}.lock').is_dir()
    binary = health.contract.readonly.binary('PD_COMPAT_GO_BIN')
    before = health.contract.readonly.snapshot()
    sources = [binary,Path(__file__),ROOT/'pkg/server/http_service_limit.rs',ROOT/'pkg/server/http_service_limit_test.rs',ROOT/'Cargo.lock']
    hashes = lambda:{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}
    env = dict(os.environ,GOFLAGS='-mod=readonly',GOWORK='off',CGO_ENABLED='0',GOPROXY='off',GOTOOLCHAIN='local')
    record = {'inputs':hashes(),'build_info':subprocess.check_output(['go','version','-m',str(binary)],env=env).decode()}
    assert 'github.com/tikv/pd' in record['build_info']
    output = target/f'http-service-limit-oracle-{time.time_ns()}.json'
    try:
        with tempfile.TemporaryDirectory(prefix='http-limit-',dir=target) as work:
            url,peer=health.address(),health.address()
            with open(Path(work)/'go.log','w+') as log:
                child = subprocess.Popen([str(binary),'--name=limit',f'--data-dir={work}/data',f'--client-urls={url}',f'--advertise-client-urls={url}',f'--peer-urls={peer}',f'--advertise-peer-urls={peer}',f'--initial-cluster=limit={peer}'],env=env,stdout=log,stderr=log,start_new_session=True)
                try:
                    deadline=time.monotonic()+40
                    while True:
                        assert child.poll() is None, 'Go exited during startup'
                        try:
                            members=request(url,'GET','members')[1]
                            if isinstance(members,dict) and members.get('leader',{}).get('member_id'):break
                        except (OSError,ValueError):pass
                        assert time.monotonic()<deadline,'Go readiness timeout'
                        time.sleep(.1)
                    record['responses']=observe(url)
                    # Incomplete body keeps the Go handler admitted; disconnect must release it.
                    address=url.removeprefix('http://').split(':')
                    pending=socket.create_connection((address[0],int(address[1])),timeout=5)
                    try:
                        pending.sendall(b'POST /pd/api/v1/service-middleware/config HTTP/1.1\r\nHost: localhost\r\nContent-Length: 999999\r\n\r\n')
                        deadline=time.monotonic()+5
                        while request(url,'POST','service-middleware/config','{}')[0] !=429:
                            assert time.monotonic()<deadline
                    finally:pending.close()
                    while request(url,'POST','service-middleware/config','{}')[0]!=200:
                        assert time.monotonic()<deadline
                    record['go_cancel_released']=True
                    record['config']=request(url,'GET','service-middleware/config')
                finally:
                    health.contract.stop(child)
                    record['go_exit']=child.returncode
                    record['go_group_clean']=not health.contract.readonly.group_alive(child.pid)
                    log.seek(0);record['go_log']=log.read()
                # Reopen the same real Go data directory: GET before shutdown alone
                # would only establish the in-memory snapshot, not durable recovery.
                restarted=subprocess.Popen(child.args,env=env,stdout=log,stderr=log,start_new_session=True)
                try:
                    deadline=time.monotonic()+40
                    while True:
                        assert restarted.poll() is None, 'Go exited during recovery'
                        try:
                            members=request(url,'GET','members')[1]
                            if isinstance(members,dict) and members.get('leader',{}).get('member_id'):break
                        except (OSError,ValueError):pass
                        assert time.monotonic()<deadline,'Go recovery readiness timeout'
                        time.sleep(.1)
                    record['recovered_config']=request(url,'GET','service-middleware/config')
                    assert record['recovered_config']==record['config']
                finally:
                    health.contract.stop(restarted)
                    record['go_recovery_exit']=restarted.returncode
                    record['go_recovery_group_clean']=not health.contract.readonly.group_alive(restarted.pid)
                    log.seek(0);record['go_log']=log.read()
            report=Path(work)/'go.json';report.write_text(json.dumps(record))
            command=['cargo','test','--locked','-p','astersql-pd-server','--lib','task_079_wire_and_cancellation','--','--nocapture']
            with tempfile.TemporaryFile(mode='w+') as log:
                child=subprocess.Popen(command,env=dict(env,PD_HTTP_SERVICE_LIMIT_ORACLE=str(report)),stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
                try:code=child.wait()
                finally:health.contract.stop(child)
                log.seek(0);text=log.read()
            record.update(command=command,rust_exit=code,rust_log=text)
            print(text)
            assert code==0 and '1 passed; 0 failed; 0 ignored' in text
    finally:
        record['readonly_unchanged']=health.contract.readonly.snapshot()==before
        record['inputs_unchanged']=hashes()==record['inputs']
        record['cleaned']=not Path(work).exists() if 'work' in locals() else True
        output.write_text(json.dumps(record,indent=2))
        print(f'oracle evidence: {output}')
        assert record['readonly_unchanged'] and record['inputs_unchanged'] and record['cleaned']

if __name__=='__main__':main()
