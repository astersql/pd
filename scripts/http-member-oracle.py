#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Existing Go binary only; real member HTTP operations and Rust comparisons."""
import importlib.util
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import urllib.request
import urllib.error
sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('health', ROOT / 'scripts/http-health-oracle.py')
health = importlib.util.module_from_spec(spec)
spec.loader.exec_module(health)
contract = health.contract

def request(url, method, path, body=''):
    req = urllib.request.Request(url + path, data=body.encode() if method == 'POST' else None, method=method)
    try:
        response = urllib.request.urlopen(req, timeout=8)
    except urllib.error.HTTPError as e:
        response = e
    with response:
        assert response.headers.get('Content-Type') == 'application/json; charset=UTF-8'
        return [response.status, json.loads(response.read())]

def main():
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, contract.interrupted)
    target = Path(os.environ['CARGO_TARGET_DIR'])
    assert target in [ROOT / f'target/rust-slot-{n}' for n in range(1, 11)]
    assert (ROOT / f'target/rust-slot-locks/slot-{target.name.rsplit("-",1)[1]}.lock').is_dir()
    binary = contract.readonly.binary('PD_COMPAT_GO_BIN')
    before = contract.readonly.snapshot()
    record = {'go_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'responses': {}}
    out = target / f'http-member-oracle-{time.time_ns()}.json'
    env = dict(os.environ, GOFLAGS='-mod=readonly', GOWORK='off', CGO_ENABLED='0', GOPROXY='off', GOTOOLCHAIN='local')
    record['build_info'] = subprocess.check_output(['go', 'version', '-m', str(binary)], env=env).decode()
    assert '\tmod\tgithub.com/tikv/pd\t' in record['build_info'] or '\tdep\tgithub.com/tikv/pd\t' in record['build_info']
    children = []
    try:
        with tempfile.TemporaryDirectory(prefix='member-oracle-', dir=target) as work:
            work = Path(work)
            urls = []
            for i in range(1, 4):
                url, peer = health.address(), health.address()
                urls.append(url)
                args = [str(binary), f'--name=member-{i}', f'--data-dir={work}/data-{i}', f'--client-urls={url}', f'--advertise-client-urls={url}', f'--peer-urls={peer}', f'--advertise-peer-urls={peer}']
                args += [f'--initial-cluster=member-1={peer}'] if i == 1 else [f'--join={urls[0]}']
                with open(work / f'go-{i}.log', 'w') as log:
                    child = subprocess.Popen(args, env=env, stdout=log, stderr=log, start_new_session=True)
                children.append(child)
                deadline = time.monotonic() + 45
                while True:
                    assert child.poll() is None, (work / f'go-{i}.log').read_text()[-5000:]
                    try:
                        response = request(urls[0], 'GET', '/pd/api/v1/members')
                        if response[0] == 200 and len(response[1].get('members', [])) == i and response[1].get('leader'):
                            break
                    except (OSError, ValueError, AssertionError):
                        pass
                    assert time.monotonic() < deadline, 'Go members readiness timeout'
                    time.sleep(.1)
                # etcd strict reconfiguration checks recent peer activity over a
                # five-second window, even after MemberList includes the joiner.
                if i > 1:
                    deadline = time.monotonic() + 20
                    while True:
                        health_result = request(urls[0], 'GET', '/pd/api/v1/health')
                        if health_result[0] == 200 and all(m['health'] for m in health_result[1]):
                            break
                        assert time.monotonic() < deadline
                        time.sleep(.1)
                    time.sleep(6)
            assert response[1]['leader']['name'] == 'member-1'
            def keep_etcd_leader_on_first():
                snapshot = request(urls[0], 'GET', '/pd/api/v1/members')[1]
                target_id = next(m['member_id'] for m in snapshot['members'] if m['name'] == 'member-1')
                endpoint = snapshot['etcd_leader']['client_urls'][0]
                if snapshot['etcd_leader']['member_id'] != target_id:
                    ctl = Path(os.environ['PD_ETCD_BIN']).with_name('etcdctl')
                    cmd = [str(ctl), f'--endpoints={endpoint}', 'move-leader', format(target_id, 'x')]
                    output = subprocess.check_output(cmd, env=env, stderr=subprocess.STDOUT).decode()
                    record.setdefault('leadership_controls', []).append({'command': cmd, 'output': output})
                deadline = time.monotonic() + 15
                while request(urls[0], 'GET', '/pd/api/v1/members')[1].get('etcd_leader', {}).get('name') != 'member-1':
                    assert time.monotonic() < deadline
                    time.sleep(.1)
            keep_etcd_leader_on_first()
            r = record['responses']
            cases = [
                ('set', 'POST', '/pd/api/v1/members/name/member-2', '{"leader-priority":7.9}'),
                ('get', 'GET', '/pd/api/v1/members', ''),
                ('bad', 'POST', '/pd/api/v1/members/name/member-2', '{"leader-priority":"bad"}'),
                ('unknown', 'POST', '/pd/api/v1/members/name/member-2', '{"ignored":true}'),
                ('null', 'POST', '/pd/api/v1/members/name/member-2', 'null'),
                ('absent', 'POST', '/pd/api/v1/members/name/absent', 'invalid'),
                ('delete_absent', 'DELETE', '/pd/api/v1/members/name/absent', ''),
                ('bad_id', 'DELETE', '/pd/api/v1/members/id/no', ''),
            ]
            for key, method, path, body in cases:
                r[key] = request(urls[0], method, path, body)
            # Priority can asynchronously move etcd leadership in Go. Reset the
            # tested priority before selecting the nonleader removal scenario.
            assert request(urls[0], 'POST', '/pd/api/v1/members/name/member-2', '{"leader-priority":0}') == [200, 'success']
            keep_etcd_leader_on_first()
            r['remove_name'] = request(urls[0], 'DELETE', '/pd/api/v1/members/name/member-2')
            assert r['remove_name'] == [200, 'removed, pd: member-2']
            def wait_names(names):
                deadline = time.monotonic() + 10
                while True:
                    members = request(urls[0], 'GET', '/pd/api/v1/members')[1]['members']
                    if {m['name'] for m in members} == names:
                        return members
                    assert time.monotonic() < deadline, members
                    time.sleep(.05)
            members = wait_names({'member-1', 'member-3'})
            keep_etcd_leader_on_first()
            member_id = next(m['member_id'] for m in members if m['name'] == 'member-3')
            r['remove_id'] = request(urls[0], 'DELETE', f'/pd/api/v1/members/id/{member_id}')
            assert r['remove_id'] == [200, f'removed, pd: {member_id}']
            r['remove_id'][1] = 'removed, pd: 3'  # Only normalize the independently verified member ID.
            record['final_members'] = wait_names({'member-1'})
            for child in children:
                contract.stop(child)
            record['go_exits'] = [child.returncode for child in children]
            children.clear()
            report = work / 'oracle.json'
            report.write_text(json.dumps(record))
            commands = [
                ['cargo', 'test', '--locked', '-p', 'astersql-pd-server', '--lib', 'task_044_readonly_go_differential', '--', '--ignored', '--nocapture'],
                ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib', 'task_044_remove_real_dedicated_member', '--', '--ignored', '--nocapture'],
            ]
            record['checks'] = []
            for cmd in commands:
                child = subprocess.Popen(cmd, env=dict(env, PD_HTTP_MEMBER_ORACLE=str(report)), stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, start_new_session=True)
                children.append(child)
                output, _ = child.communicate()
                record['checks'].append({'command': cmd, 'exit': child.returncode, 'output': output})
                print(output[-4000:])
                assert child.returncode == 0 and '1 passed; 0 failed; 0 ignored' in output
    finally:
        for child in children:
            contract.stop(child)
        record['cleaned'] = not work.exists() if 'work' in locals() else True
        record['readonly_unchanged'] = contract.readonly.snapshot() == before
        out.write_text(json.dumps(record, indent=2))
        print(f'oracle evidence: {out}')
        assert record['readonly_unchanged']
def replay():
    """Replay a preserved successful Go capture against the current Rust sources."""
    target = Path(os.environ['CARGO_TARGET_DIR'])
    assert target in [ROOT / f'target/rust-slot-{n}' for n in range(1, 11)]
    assert (ROOT / f'target/rust-slot-locks/slot-{target.name.rsplit("-",1)[1]}.lock').is_dir()
    source = Path(os.environ['PD_HTTP_MEMBER_REPLAY']).resolve()
    captured = source.read_bytes()
    record = json.loads(captured)
    binary = contract.readonly.binary('PD_COMPAT_GO_BIN')
    assert record['go_sha256'] == hashlib.sha256(binary.read_bytes()).hexdigest()
    assert record['readonly_unchanged']
    assert record['responses']['remove_name'] == [200, 'removed, pd: member-2']
    assert record['responses']['remove_id'] == [200, 'removed, pd: 3']
    assert [m['name'] for m in record['final_members']] == ['member-1']
    record['capture_checks'] = record.pop('checks', [])
    record['replay_source'] = {'path': str(source), 'sha256': hashlib.sha256(captured).hexdigest()}
    record['checks'] = []
    before = contract.readonly.snapshot()
    env = dict(os.environ, GOFLAGS='-mod=readonly', GOWORK='off', CGO_ENABLED='0', GOPROXY='off', GOTOOLCHAIN='local')
    record['build_info'] = subprocess.check_output(['go', 'version', '-m', str(binary)], env=env).decode()
    out = target / f'http-member-oracle-{time.time_ns()}.json'
    child = None
    try:
        with tempfile.TemporaryDirectory(prefix='member-replay-', dir=target) as directory:
            report = Path(directory) / 'go.json'
            report.write_text(json.dumps(record))
            for package, test in [('astersql-pd-server', 'task_044_readonly_go_differential'), ('astersql-pd-runtime', 'task_044_remove_real_dedicated_member')]:
                cmd = ['cargo', 'test', '--locked', '-p', package, '--lib', test, '--', '--ignored', '--nocapture']
                child = subprocess.Popen(cmd, env=dict(env, PD_HTTP_MEMBER_ORACLE=str(report)), stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, start_new_session=True)
                output, _ = child.communicate()
                record['checks'].append({'command': cmd, 'exit': child.returncode, 'output': output})
                print(output[-2500:])
                assert child.returncode == 0 and '1 passed; 0 failed; 0 ignored' in output
    finally:
        if child is not None:
            contract.stop(child)
        record['readonly_unchanged'] = contract.readonly.snapshot() == before
        record['cleaned'] = not Path(directory).exists() if 'directory' in locals() else True
        out.write_text(json.dumps(record, indent=2))
        print(f'oracle evidence: {out}')
        assert record['readonly_unchanged'] and record['cleaned']

if __name__ == '__main__':
    if os.environ.get('PD_HTTP_MEMBER_REPLAY'):
        replay()
    else:
        main()
