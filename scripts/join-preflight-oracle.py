#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Task088: existing Go process vs real external-MetaStore Rust admission checks."""
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
    target = Path(os.environ['CARGO_TARGET_DIR'])
    assert target in [ROOT / f'target/rust-slot-{n}' for n in range(1, 11)]
    assert (ROOT / f"target/rust-slot-locks/slot-{target.name.rsplit('-', 1)[1]}.lock").is_dir()
    binary = ro.binary('PD_COMPAT_GO_BIN')
    before = ro.snapshot()
    record = {'go_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'build_info': subprocess.check_output(['go', 'version', '-m', str(binary)]).decode()}
    assert 'github.com/tikv/pd' in record['build_info']
    children, logs = [], []
    def interrupted(*_):
        raise KeyboardInterrupt()
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    def address():
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            return f'http://127.0.0.1:{sock.getsockname()[1]}'
    def members(url):
        output = subprocess.check_output(['curl', '-fsS', '--max-time', '2', url + '/pd/api/v1/members'], stderr=subprocess.DEVNULL)
        return json.loads(output)['members']
    def start(label, name, client, peer, join=None, data=None):
        log = open(work / f'{label}.log', 'w+')
        logs.append((label, log))
        command = [str(binary), f'--name={name}', f'--data-dir={data or work / label}',
                   f'--client-urls={client}', f'--advertise-client-urls={client}',
                   f'--peer-urls={peer}', f'--advertise-peer-urls={peer}']
        command += [f'--join={join}', '--initial-cluster='] if join else [f'--initial-cluster={name}={peer}']
        record.setdefault('go_commands', []).append(command)
        child = subprocess.Popen(command, stdout=log, stderr=log, start_new_session=True)
        children.append(child)
        return child, log
    def ready(child, url):
        deadline = time.monotonic() + 45
        while time.monotonic() < deadline:
            assert child.poll() is None, f'Go exited: {child.returncode}'
            try:
                result = members(url)
                if result: return result
            except (subprocess.SubprocessError, ValueError, KeyError):
                pass
            time.sleep(.1)
        raise AssertionError('Go readiness timeout')
    def stop(child):
        if child.poll() is None:
            os.killpg(child.pid, signal.SIGTERM)
            try: child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait(timeout=5)
    output = target / 'task088-oracle.json'
    try:
        with tempfile.TemporaryDirectory(prefix='join88-', dir=target) as directory:
            work = Path(directory)
            try:
                leader_url, leader_peer = address(), address()
                leader, _ = start('leader', 'existing', leader_url, leader_peer)
                initial = ready(leader, leader_url)
                cases = [
                    ('self', 'self', address(), address(), 'join self is forbidden'),
                    ('name', 'existing', address(), address(), 'missing data or join a duplicated pd'),
                    ('url', 'duplicate-url', leader_url, address(), 'already used by member'),
                    ('url-canonical', 'canonical-url', leader_url.rsplit(':', 1)[0] + ':0' + leader_url.rsplit(':', 1)[1], address(), 'already used by member'),
                ]
                record['rejections'] = []
                for label, name, client, peer, expected in cases:
                    join = client if label == 'self' else (leader_url + ',' + leader_url if label == 'url' else leader_url)
                    child, log = start(label, name, client, peer, join)
                    code = child.wait(timeout=20)
                    log.flush(); log.seek(0); text = log.read()
                    assert code != 0 and expected in text, (label, code, text)
                    assert members(leader_url) == initial, 'rejected join mutated member list'
                    record['rejections'].append({'case': label, 'exit': code, 'expected': expected, 'members_unchanged': True})
                follower_url, follower_peer = address(), address()
                follower, _ = start('follower', 'fresh', follower_url, follower_peer, leader_url)
                assert len(ready(follower, follower_url)) == 2
                record['success_member_count'] = 2
                stop(follower)
                follower, _ = start('restart', 'fresh', follower_url, follower_peer, leader_url, work / 'follower')
                assert len(ready(follower, follower_url)) == 2
                record['go_restart_with_data_and_join'] = 'accepted; embedded-etcd boundary, not external-MetaStore equivalence'
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib', 'task_088_', '--', '--nocapture']
                rust = subprocess.Popen(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, start_new_session=True)
                children.append(rust)
                text, _ = rust.communicate()
                record.update(rust_command=command, rust_exit=rust.returncode, rust_log=text)
                print(text)
                assert rust.returncode == 0 and '4 passed; 0 failed; 0 ignored' in text
            finally:
                for child in reversed(children): stop(child)
                record['processes_reaped'] = all(child.poll() is not None for child in children)
                record['logs'] = {}
                for label, log in logs:
                    log.flush(); log.seek(0); record['logs'][label] = log.read(); log.close()
        record['temporary_directory_removed'] = not work.exists()
    finally:
        record['readonly_unchanged'] = before == ro.snapshot()
        output.write_text(json.dumps(record, ensure_ascii=False, indent=2))
        print(output)
        assert record['readonly_unchanged']


if __name__ == '__main__':
    main()
