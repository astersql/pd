#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Read-only Go binary oracle for real consensus transfer and PD lease turnover."""
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
spec = importlib.util.spec_from_file_location('member', ROOT / 'scripts/http-member-oracle.py')
member = importlib.util.module_from_spec(spec)
spec.loader.exec_module(member)
contract = member.contract


def request(url, method, path):
    req = urllib.request.Request(url + path, data=b'' if method == 'POST' else None, method=method)
    try:
        response = urllib.request.urlopen(req, timeout=25)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        assert response.headers.get('Content-Type') == 'application/json; charset=UTF-8'
        return [response.status, json.loads(response.read())]


def main():
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, contract.interrupted)
    target = Path(os.environ['CARGO_TARGET_DIR'])
    assert target in [ROOT / f'target/rust-slot-{n}' for n in range(1, 11)]
    assert (ROOT / f'target/rust-slot-locks/slot-{target.name.rsplit("-", 1)[1]}.lock').is_dir()
    binary = contract.readonly.binary('PD_COMPAT_GO_BIN')
    before = contract.readonly.snapshot()
    record = {'go_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'responses': {}}
    output = target / f'http-leader-oracle-{time.time_ns()}.json'
    env = dict(os.environ, GOFLAGS='-mod=readonly', GOWORK='off', CGO_ENABLED='0', GOPROXY='off', GOTOOLCHAIN='local')
    record['build_info'] = subprocess.check_output(['go', 'version', '-m', str(binary)], env=env).decode()
    assert '\tmod\tgithub.com/tikv/pd\t' in record['build_info'] or '\tdep\tgithub.com/tikv/pd\t' in record['build_info']
    children = []
    try:
        with tempfile.TemporaryDirectory(prefix='leader-oracle-', dir=target) as directory:
            work = Path(directory)
            urls = []
            for i in range(1, 4):
                url, peer = member.health.address(), member.health.address()
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
                        r = request(urls[0], 'GET', '/pd/api/v1/members')
                        if r[0] == 200 and len(r[1]['members']) == i and r[1].get('leader'):
                            break
                    except (OSError, ValueError, AssertionError, KeyError):
                        pass
                    assert time.monotonic() < deadline
                    time.sleep(.1)
                if i > 1:
                    time.sleep(6)
            cluster = r[1]['header']['cluster_id']
            def lease(url):
                ctl = Path(os.environ['PD_ETCD_BIN']).with_name('etcdctl')
                cmd = [str(ctl), f'--endpoints={url}', 'get', f'/pd/{cluster}/leader', '-w', 'json']
                result = json.loads(subprocess.check_output(cmd, env=env))
                record.setdefault('leases', []).append(result)
                return result['kvs'][0]['lease']
            def wait_leader(name=None, excluded=None):
                deadline = time.monotonic() + 20
                while True:
                    r = request(urls[0], 'GET', '/pd/api/v1/members')[1]
                    leader = r.get('leader', {})
                    if leader and leader['name'] != excluded and (name is None or leader['name'] == name) and r.get('etcd_leader', {}).get('name') == leader['name']:
                        return leader
                    assert time.monotonic() < deadline, r
                    time.sleep(.1)
            leader = wait_leader()
            old = int(leader['name'].split('-')[-1]) - 1
            next_id = (old + 1) % 3
            r = record['responses']
            r['get'] = request(urls[old], 'GET', '/pd/api/v1/leader')
            assert r['get'][1]['member_id'] == leader['member_id']
            r['self'] = request(urls[old], 'POST', f'/pd/api/v1/leader/transfer/{leader["name"]}')
            old_lease = lease(urls[old])
            r['missing'] = request(urls[old], 'POST', '/pd/api/v1/leader/transfer/absent')
            assert lease(urls[old]) == old_lease
            r['transfer'] = request(urls[old], 'POST', f'/pd/api/v1/leader/transfer/member-{next_id+1}')
            assert r['transfer'] == [200, 'The transfer command is submitted.']
            leader = wait_leader(f'member-{next_id+1}')
            record['lease_changed'] = lease(urls[next_id]) != old_lease
            assert record['lease_changed']
            r['resign'] = request(urls[next_id], 'POST', '/pd/api/v1/leader/resign')
            assert r['resign'] == [200, 'The resign command is submitted.']
            leader = wait_leader(excluded=f'member-{next_id+1}')
            current = int(leader['name'].split('-')[-1])-1
            dead = (current+1)%3
            contract.stop(children[dead])
            before_dead = lease(urls[current])
            r['dead'] = request(urls[current], 'POST', f'/pd/api/v1/leader/transfer/member-{dead+1}')
            assert r['dead'][0] == 500 and 'ErrEtcdMoveLeader' in r['dead'][1]
            assert lease(urls[current]) == before_dead
            for child in children:
                contract.stop(child)
            record['go_exits'] = [child.returncode for child in children]
            children.clear()
            report = work / 'go.json'
            report.write_text(json.dumps(record))
            cmd = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib', 'task_045_real_transfer_and_fencing', '--', '--ignored', '--nocapture']
            child = subprocess.Popen(cmd, env=dict(env, PD_HTTP_LEADER_ORACLE=str(report)), stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, start_new_session=True)
            children.append(child)
            text, _ = child.communicate()
            record['check'] = {'command': cmd, 'exit': child.returncode, 'output': text}
            print(text[-4000:])
            assert child.returncode == 0 and '1 passed; 0 failed; 0 ignored' in text
    finally:
        for child in children:
            contract.stop(child)
        record['readonly_unchanged'] = contract.readonly.snapshot() == before
        record['cleaned'] = not Path(directory).exists() if 'directory' in locals() else True
        output.write_text(json.dumps(record, indent=2))
        print(f'oracle evidence: {output}')
        assert record['readonly_unchanged'] and record['cleaned']


if __name__ == '__main__':
    main()
