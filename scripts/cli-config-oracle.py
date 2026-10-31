#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Compare file/CLI validation using existing Go and slot-owned Rust binaries."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import signal
import socket
import time
import urllib.request
import sys
import tempfile

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('readonly', ROOT / 'scripts/pd-compat-readonly.py')
readonly = importlib.util.module_from_spec(spec)
spec.loader.exec_module(readonly)


def runtime_pair(go, rust, target, directory, env, shutdown=signal.SIGTERM):
    """Observe merged identity over real HTTP and signal-driven shutdown."""
    fixtures = sorted(target.glob('rust-etcd-fixture-*'), key=lambda p: p.stat().st_mtime)
    assert fixtures, 'package tests must first build the read-only etcd fixture'
    fixture_binary = fixtures[-1]
    observations = []
    for kind, binary in [('go', go), ('rust', rust)]:
        work = Path(directory) / kind
        work.mkdir()
        children = []
        try:
            extra = []
            if kind == 'rust':
                fixture_dir = work / 'fixture'
                fixture_dir.mkdir()
                log = (work / 'fixture.log').open('wb')
                fixture = subprocess.Popen([str(fixture_binary), str(fixture_dir)], stdin=subprocess.PIPE,
                                           stdout=log, stderr=log, env=env, start_new_session=True)
                log.close()
                children.append(fixture)
                deadline = time.monotonic() + 20
                while not (fixture_dir / 'ready').exists():
                    assert fixture.poll() is None and time.monotonic() < deadline
                    time.sleep(0.05)
                extra = ['--etcd-endpoints=' + (fixture_dir / 'ready').read_text()]
            else:
                extra = ['--data-dir=' + str(work / 'data')]
            def url():
                with socket.socket() as listener:
                    listener.bind(('127.0.0.1', 0))
                    return f'http://127.0.0.1:{listener.getsockname()[1]}'
            client, peer = url(), url()
            config = work / 'config.toml'
            config.write_text(f"name='from-file'\nclient-urls='{client}'\npeer-urls='{peer}'\n")
            log = (work / 'server.log').open('wb')
            argv = [str(binary), '--config', str(config), '--name=from-cli', *extra]
            server = subprocess.Popen(argv, stdout=log, stderr=log, env=env, start_new_session=True)
            log.close()
            children.append(server)
            deadline = time.monotonic() + 30
            while True:
                assert server.poll() is None, (kind, (work / 'server.log').read_text())
                try:
                    with urllib.request.urlopen(client + '/pd/api/v1/members', timeout=1) as response:
                        members = json.load(response)
                    if members.get('members'):
                        break
                except (OSError, ValueError):
                    pass
                assert time.monotonic() < deadline
                time.sleep(0.05)
            member = members['members'][0]
            assert member['name'] == 'from-cli' and member['client_urls'] == [client]
            assert member['peer_urls'] == [peer]
            server.send_signal(shutdown)
            assert server.wait(timeout=15) == (0 if shutdown == signal.SIGTERM else 1)
            with socket.socket() as listener:
                listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
                listener.bind(('127.0.0.1', int(client.rsplit(':', 1)[1])))
            observations.append({'kind': kind, 'argv': argv, 'members': members, 'exit': server.returncode,
                                 'listener_released': True, 'log': (work / 'server.log').read_text()})
        finally:
            for child in reversed(children):
                if child.poll() is None:
                    if child.stdin:
                        child.stdin.close()
                        try:
                            child.wait(timeout=15)
                        except subprocess.TimeoutExpired:
                            readonly.cleanup(child)
                    else:
                        readonly.cleanup(child)
                assert not readonly.group_alive(child.pid)
    return {'observations': observations, 'fixture_sha256': hashlib.sha256(fixture_binary.read_bytes()).hexdigest()}


def main():
    target = Path(os.environ['CARGO_TARGET_DIR'])
    assert target in [ROOT / f'target/rust-slot-{n}' for n in range(1, 11)]
    assert (ROOT / f'target/rust-slot-locks/slot-{target.name.rsplit("-", 1)[1]}.lock').is_dir()
    go = readonly.binary('PD_COMPAT_GO_BIN')
    rust = target / 'debug/pd-server'
    before = readonly.snapshot()
    record = {'go_sha256': hashlib.sha256(go.read_bytes()).hexdigest(),
              'rust_sha256': hashlib.sha256(rust.read_bytes()).hexdigest(),
              'go_build_info': subprocess.check_output(['go', 'version', '-m', str(go)]).decode(),
              'cases': []}
    env = dict(os.environ, GOFLAGS='-mod=readonly', GOWORK='off', GOPROXY='off',
               GOTOOLCHAIN='local', CGO_ENABLED='0', PD_NAME='ignored-environment-name')
    cases = [
        ('defaults', '', [], 'config check successful'),
        ('file-and-cli', "name='file'\nclient-urls='http://127.0.0.1:2356'", ['--name', 'cli'], 'config check successful'),
        ('deprecated', "log-level='debug'\n[log]\nlevel='warn'", ['-L', 'error'], 'deprecated'),
        ('nested-log-path', "data-dir='data90'\n[log.file]\nfilename='data90/sub/pd.log'", [], "log directory shouldn't"),
        ('cli-overrides-file', "data-dir='data90'\n[log.file]\nfilename='data90/sub/pd.log'", ['--log-file=outside90.log'], 'config check successful'),
        ('empty-does-not-override', "data-dir='data90'\n[log.file]\nfilename='data90/sub/pd.log'", ['--log-file='], "log directory shouldn't"),
        ('join-conflict', "initial-cluster='pd=http://127.0.0.1:2380'", ['--join=http://127.0.0.1:2379'], 'can not be provided at the same time'),
        ('true-file-false-cli', "ForceNewCluster=true", ['--force-new-cluster=false'], 'config check successful'),
        ('malformed', 'name=[', [], None),
    ]
    try:
        with tempfile.TemporaryDirectory(prefix='pd-cli90-oracle-') as directory:
            for name, text, extra, expected in cases:
                path = Path(directory) / f'{name}.toml'
                path.write_text(text)
                outputs = []
                for binary in (go, rust):
                    argv = [str(binary), '--config-check', '--config', str(path), *extra]
                    result = subprocess.run(argv, env=env, capture_output=True, text=True, timeout=20)
                    output = result.stdout + result.stderr
                    outputs.append({'argv': argv, 'exit': result.returncode, 'output': output})
                    if expected is not None:
                        assert expected in output, (name, str(binary), output)
                    else:
                        assert 'config check successful' not in output and output.strip(), (name, output)
                record['cases'].append({'name': name, 'processes': outputs})
            record['runtime'] = runtime_pair(go, rust, target, directory, env)
            record['temporary_directory'] = directory
        record['temporary_directory_cleaned'] = not Path(directory).exists()
    finally:
        record['readonly_unchanged'] = before == readonly.snapshot()
        report = Path(os.environ.get('PD_CLI_CONFIG_REPORT', str(target / 'cli-config-oracle.json')))
        report.write_text(json.dumps(record, indent=2))
        print(report)
    assert len(record['cases']) == len(cases) and record['readonly_unchanged']
    print(f"{len(cases)} real Go/Rust validation pairs passed")


if __name__ == '__main__':
    main()
