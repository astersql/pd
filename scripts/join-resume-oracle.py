#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Read-only Go recovery oracle using the existing PD binary, without failpoints."""
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

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent


def main():
    spec = importlib.util.spec_from_file_location('readonly', ROOT / 'scripts/pd-compat-readonly.py')
    ro = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(ro)
    before = ro.snapshot()
    binary = ro.binary('PD_COMPAT_GO_BIN')
    record = {'go_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'build_info': subprocess.check_output(['go', 'version', '-m', str(binary)]).decode()}
    assert 'github.com/tikv/pd' in record['build_info']
    children, logs = [], []

    def interrupted(*_):
        raise KeyboardInterrupt()

    signal.signal(signal.SIGINT, interrupted)
    signal.signal(signal.SIGTERM, interrupted)

    def address():
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            return f'http://127.0.0.1:{sock.getsockname()[1]}'

    def call(url, path, body=None):
        command = ['curl', '-fsS', '--max-time', '3', url + path]
        if body is not None:
            command += ['-X', 'POST', '-H', 'Content-Type: application/json', '-d', json.dumps(body)]
        return json.loads(subprocess.check_output(command, stderr=subprocess.DEVNULL))

    def start(label, name, client, peer, join=None):
        log = open(work / f'{label}.log', 'w+')
        logs.append((label, log))
        command = [str(binary), f'--name={name}', f'--data-dir={work / label}',
                   f'--client-urls={client}', f'--advertise-client-urls={client}',
                   f'--peer-urls={peer}', f'--advertise-peer-urls={peer}']
        command += [f'--join={join}', '--initial-cluster='] if join else [f'--initial-cluster={name}={peer}']
        record.setdefault('commands', []).append(command)
        child = subprocess.Popen(command, stdout=log, stderr=log, start_new_session=True)
        children.append(child)
        return child, log

    def ready(child, url):
        deadline = time.monotonic() + 45
        while time.monotonic() < deadline:
            assert child.poll() is None, f'Go exited: {child.returncode}'
            try:
                members = call(url, '/pd/api/v1/members')['members']
                if members:
                    return members
            except (subprocess.SubprocessError, ValueError, KeyError):
                pass
            time.sleep(.1)
        raise AssertionError('Go readiness timeout')

    try:
        with tempfile.TemporaryDirectory(prefix='join89-') as directory:
            work = Path(directory)
            try:
                leader_url, leader_peer = address(), address()
                leader, _ = start('leader', 'existing', leader_url, leader_peer)
                assert len(ready(leader, leader_url)) == 1
                follower_url, follower_peer = address(), address()
                # Reproduce the persisted boundary after MemberAdd but before PD starts.
                added = call(leader_url, '/v3/cluster/member/add', {'peerURLs': [follower_peer]})
                member_id = int(added['member']['ID'])
                assert len(added['members']) == 2
                assert not added['member'].get('name')
                record['unfinished_member'] = added
                follower, log = start('recovery', 'recover', follower_url, follower_peer, leader_url)
                members = ready(follower, follower_url)
                assert len(members) == 2
                recovered = next(m for m in members if m['name'] == 'recover')
                assert recovered['member_id'] == member_id, (recovered, member_id)
                record['recovery'] = {'members': members, 'same_member_id': True}
                duplicate, dup_log = start('duplicate', 'recover', address(), address(), leader_url)
                code = duplicate.wait(timeout=20)
                dup_log.flush(); dup_log.seek(0)
                assert code != 0 and 'missing data or join a duplicated pd' in dup_log.read()
                assert call(follower_url, '/pd/api/v1/members')['members'] == members
                record['duplicate'] = {'exit': code, 'members_unchanged': True}
                log.flush(); log.seek(0)
                assert 'previously failed to start after join' in log.read()
            finally:
                for child in reversed(children):
                    if child.poll() is None:
                        os.killpg(child.pid, signal.SIGTERM)
                        try:
                            child.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            os.killpg(child.pid, signal.SIGKILL)
                            child.wait(timeout=5)
                record['processes_reaped'] = all(child.poll() is not None for child in children)
                record['logs'] = {}
                for label, log in logs:
                    log.flush(); log.seek(0)
                    record['logs'][label] = log.read()
                    log.close()
        record['temporary_directory_removed'] = not work.exists()
    finally:
        record['readonly_unchanged'] = before == ro.snapshot()
        Path(os.environ['PD_JOIN_RESUME_REPORT']).write_text(json.dumps(record, ensure_ascii=False, indent=2))
        assert record['readonly_unchanged']
    print('Go recovery retained member ID; duplicate rejected; all processes reaped')


if __name__ == '__main__':
    main()
