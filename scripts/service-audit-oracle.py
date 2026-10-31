#!/usr/bin/env python3
"""Read-only Go audit route/enablement differential; no Go build or injection."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request
sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('limit', ROOT/'scripts/http-service-limit-oracle.py')
limit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(limit)
health = limit.health


def metrics(url):
    text = urllib.request.urlopen(url+'/metrics', timeout=5).read().decode()
    return {name: sum(int(float(value)) for labels, value in re.findall(r'^pd_service_audit_requests_total\{([^}]+)\} (\S+)$', text, re.M) if f'service="{name}"' in labels)
            for name in ['Ping', 'SetServiceMiddlewareConfig']}


def durations(url):
    text = urllib.request.urlopen(url+'/metrics', timeout=5).read().decode()
    return {name: {kind: sum(float(value) for metric, labels, value in re.findall(r'^(pd_service_audit_handling_seconds_(?:sum|count))\{([^}]+)\} (\S+)$', text, re.M)
                            if metric.endswith('_'+kind) and f'service="{name}"' in labels)
                   for kind in ['sum', 'count']}
            for name in ['Ping', 'SetServiceMiddlewareConfig']}


def main():
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, health.contract.interrupted)
    target = Path(os.environ['CARGO_TARGET_DIR'])
    assert target in [ROOT/f'target/rust-slot-{n}' for n in range(1, 11)]
    assert (ROOT/f'target/rust-slot-locks/slot-{target.name.rsplit("-", 1)[1]}.lock').is_dir()
    binary = health.contract.readonly.binary('PD_COMPAT_GO_BIN')
    before = health.contract.readonly.snapshot()
    sources = [binary, Path(__file__), ROOT/'pkg/server/service_audit.rs', ROOT/'pkg/server/service_audit_test.rs', ROOT/'pkg/server/http_service_limit.rs', ROOT/'pkg/server/lib.rs', ROOT/'Cargo.lock']
    hashes = lambda: {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}
    env = dict(os.environ, GOFLAGS='-mod=readonly', GOWORK='off', CGO_ENABLED='0', GOPROXY='off', GOTOOLCHAIN='local')
    record = {'inputs': hashes(), 'build_info': subprocess.check_output(['go', 'version', '-m', str(binary)], env=env).decode()}
    assert 'github.com/tikv/pd' in record['build_info']
    output = target/f'service-audit-oracle-{time.time_ns()}.json'
    try:
        with tempfile.TemporaryDirectory(prefix='audit-', dir=target) as work:
            url, peer = health.address(), health.address()
            with open(Path(work)/'go.log', 'w+') as log:
                child = subprocess.Popen([str(binary), '--name=audit', f'--data-dir={work}/data', f'--client-urls={url}', f'--advertise-client-urls={url}', f'--peer-urls={peer}', f'--advertise-peer-urls={peer}', f'--initial-cluster=audit={peer}'], env=env, stdout=log, stderr=log, start_new_session=True)
                try:
                    deadline = time.monotonic()+40
                    while True:
                        assert child.poll() is None
                        try:
                            members = limit.request(url, 'GET', 'members')[1]
                            if isinstance(members, dict) and members.get('leader', {}).get('member_id'):
                                break
                        except (OSError, ValueError):
                            pass
                        assert time.monotonic() < deadline
                        time.sleep(.1)
                    initial = metrics(url)
                    initial_duration = durations(url)
                    responses = []
                    counts = []
                    for method, path, body in [
                        ('GET', 'ping?token=secret', ''),
                        ('POST', 'service-middleware/config?password=secret', 'secret'),
                        ('POST', 'service-middleware/config', '{"audit.enable-audit":"false"}'),
                        ('GET', 'ping', ''),
                        ('POST', 'service-middleware/config', '{"audit.enable-audit":"true"}'),
                        ('GET', 'ping', ''),
                        ('GET', 'unknown-secret', ''),
                    ]:
                        responses.append(limit.request(url, method, path, body))
                        counts.append({k: v-initial[k] for k, v in metrics(url).items()})
                    record.update(responses=responses, counts=counts)
                    assert [r[0] for r in responses] == [200, 400, 200, 200, 200, 200, 404], responses
                    assert counts == [dict(Ping=p, SetServiceMiddlewareConfig=c) for p,c in [(1,0),(1,1),(1,2),(1,2),(1,2),(2,2),(2,2)]], counts
                    record['config'] = limit.request(url, 'GET', 'service-middleware/config')
                    # Go captures local-log bodies before dispatch; abort a partial body
                    # and require exactly one completed audit, without failpoints.
                    host, port = url.removeprefix('http://').split(':')
                    pending = socket.create_connection((host, int(port)), timeout=5)
                    pending.sendall(b'POST /pd/api/v1/service-middleware/config HTTP/1.1\r\nHost: localhost\r\nContent-Length: 999999\r\n\r\nsecret')
                    pending.close()
                    deadline = time.monotonic()+5
                    while metrics(url)['SetServiceMiddlewareConfig'] - initial['SetServiceMiddlewareConfig'] != 3:
                        assert time.monotonic() < deadline
                        time.sleep(.01)
                    record['cancel_audit_delta'] = 1
                    record['go_duration_initial'] = initial_duration
                    record['go_duration_final'] = durations(url)
                    for name, count in [('Ping', 2), ('SetServiceMiddlewareConfig', 3)]:
                        assert record['go_duration_final'][name]['count']-initial_duration[name]['count'] == count
                        assert record['go_duration_final'][name]['sum'] >= initial_duration[name]['sum']
                    log.seek(0)
                    record['primary_go_log'] = log.read()
                    assert limit.request(url, 'POST', 'service-middleware/config?phase=recovery', '{"audit.enable-audit":"false"}')[0] == 200



                finally:
                    health.contract.stop(child)
                    record['go_exit'] = child.returncode
                    record['go_group_clean'] = not health.contract.readonly.group_alive(child.pid)
                    log.seek(0)
                    record['go_log'] = log.read()
                # Local-log selection is observable, with both error and success audited.
                audit_lines = [line for line in record['primary_go_log'].splitlines() if 'audit log' in line and 'ServiceLabel:SetServiceMiddlewareConfig,' in line]
                assert len(audit_lines) == 3, audit_lines
                record['local_log_count'] = len(audit_lines)-record['cancel_audit_delta']
                assert not any('audit log' in line and 'ServiceLabel:Ping,' in line for line in record['go_log'].splitlines())
                # Reopen the same durable Go directory with audit explicitly disabled.
                restarted = subprocess.Popen(child.args, env=env, stdout=log, stderr=log, start_new_session=True)
                try:
                    deadline = time.monotonic()+40
                    while True:
                        assert restarted.poll() is None
                        try:
                            members = limit.request(url, 'GET', 'members')[1]
                            if isinstance(members, dict) and members.get('leader', {}).get('member_id'):
                                break
                        except (OSError, ValueError):
                            pass
                        assert time.monotonic() < deadline
                        time.sleep(.1)
                    record['recovered_config'] = limit.request(url, 'GET', 'service-middleware/config')
                    assert record['recovered_config'][1]['audit']['enable-audit'] == 'false'
                    assert limit.request(url, 'GET', 'ping')[0] == 200
                    assert metrics(url)['Ping'] == 0
                finally:
                    health.contract.stop(restarted)
                    record['go_recovery_group_clean'] = not health.contract.readonly.group_alive(restarted.pid)
                    log.seek(0)
                    record['go_log'] = log.read()

            report = Path(work)/'go.json'
            report.write_text(json.dumps(record))
            command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-server', '--lib', 'task_081_behavior', '--', '--nocapture']
            with tempfile.TemporaryFile(mode='w+') as log:
                rust = subprocess.Popen(command, env=dict(env, PD_AUDIT_GO_REPORT=str(report)), stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
                try:
                    code = rust.wait()
                finally:
                    health.contract.stop(rust)
                    record["rust_group_clean"] = not health.contract.readonly.group_alive(rust.pid)
                log.seek(0)
                record.update(command=command, rust_exit=code, rust_log=log.read())
            print(record['rust_log'])
            assert code == 0 and '1 passed; 0 failed; 0 ignored' in record['rust_log']
    finally:
        record['readonly_unchanged'] = health.contract.readonly.snapshot() == before
        record['inputs_unchanged'] = hashes() == record['inputs']
        record['cleaned'] = not Path(work).exists() if 'work' in locals() else True
        output.write_text(json.dumps(record, indent=2))
        print(f'oracle evidence: {output}')
        assert record['readonly_unchanged'] and record['inputs_unchanged'] and record['cleaned']
        assert all(record.get(key, True) for key in ['go_group_clean', 'go_recovery_group_clean', 'rust_group_clean'])


if __name__ == '__main__':
    main()
