#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Read-only Go/Rust logging process and admin/log observations."""
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
spec = importlib.util.spec_from_file_location('config', ROOT / 'scripts/cli-config-oracle.py')
config = importlib.util.module_from_spec(spec)
spec.loader.exec_module(config)


def url():
    with socket.socket() as s:
        s.bind(('127.0.0.1', 0))
        return f'http://127.0.0.1:{s.getsockname()[1]}'


def request(base, body):
    req = urllib.request.Request(base + '/pd/api/v1/admin/log', data=body.encode(), method='POST')
    try:
        response = urllib.request.urlopen(req, timeout=5)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        return [response.status, json.loads(response.read())]


def observe(kind, binary, target, directory, env):
    work = directory / kind
    work.mkdir()
    children = []
    try:
        if kind == 'rust':
            child, _, endpoint = config.readonly.start_storage_fixture(target, work / 'fixture', env)
            children.append(child)
            extra = ['--etcd-endpoints=' + endpoint]
        else:
            extra = ['--data-dir=' + str(work / 'data')]
        client, peer = url(), url()
        log_file = work / 'pd.log'
        cfg = work / 'config.toml'
        cfg.write_text(f"name='logging-oracle'\nclient-urls='{client}'\npeer-urls='{peer}'\n[log]\nlevel='info'\nformat='json'\ndisable-timestamp=true\n[log.file]\nfilename='{log_file}'\n")
        argv = [str(binary), '--config=' + str(cfg), *extra]
        with (work / 'stdout').open('wb') as out, (work / 'stderr').open('wb') as err:
            child = subprocess.Popen(argv, stdout=out, stderr=err, env=env, start_new_session=True)
        children.append(child)
        deadline = time.monotonic() + 40
        while True:
            assert child.poll() is None, (kind, (work / 'stdout').read_text(), log_file.read_text() if log_file.exists() else '')
            try:
                with urllib.request.urlopen(client + '/pd/api/v1/members', timeout=1) as response:
                    members = json.load(response)
                if members.get('members'):
                    break
            except (OSError, ValueError):
                pass
            assert time.monotonic() < deadline
            time.sleep(0.05)
        bodies = ['"debug"', '"DeBuG"', '"trace"', '"warning"', '"WARN"', '"fatal"', '"error"', '"info"', 'null', '{}', '12', '[1]', '""', '"unterminated', '', '"debug"']
        bodies.append(json.dumps("x" * (1024 * 1024)))
        observations = [[body, request(client, body)] for body in bodies]
        # All expected warning events appear except while fatal/error thresholds apply.
        child.send_signal(signal.SIGTERM)
        assert child.wait(timeout=20) == 0
        with socket.socket() as s:
            s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            s.bind(('127.0.0.1', int(client.rsplit(':', 1)[1])))
        text = log_file.read_text()
        events = [json.loads(line) for line in text.splitlines() if line.strip()]
        changes = [e for e in events if e.get('msg', e.get('message', e.get('fields', {}).get('message'))) == 'log level changed']
        assert len(changes) == 6, (kind, text)
        assert cfg.read_text().find("level='info'") >= 0
        return dict(kind=kind, argv=argv, responses=observations, log=text,
                    stdout=(work / 'stdout').read_text(), stderr=(work / 'stderr').read_text(),
                    members=members, exit=child.returncode, listener_released=True, changes=len(changes))
    finally:
        for child in reversed(children):
            if child.poll() is None:
                if child.stdin:
                    child.stdin.close()
                    try:
                        child.wait(timeout=15)
                    except subprocess.TimeoutExpired:
                        config.readonly.cleanup(child)
                else:
                    config.readonly.cleanup(child)
            assert not config.readonly.group_alive(child.pid)


def main():
    target = Path(os.environ['CARGO_TARGET_DIR']).resolve()
    assert target in [ROOT / f'target/rust-slot-{n}' for n in range(1, 11)]
    assert (ROOT / f'target/rust-slot-locks/slot-{target.name.rsplit("-", 1)[1]}.lock').is_dir()
    binaries = [('go', config.readonly.binary('PD_COMPAT_GO_BIN')), ('rust', target / 'debug/pd-server')]
    before = config.readonly.snapshot()
    record = dict(before=before, target=str(target), binaries={k: {'path': str(p), 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for k, p in binaries})
    env = dict(os.environ, GOFLAGS='-mod=readonly', GOWORK='off', GOPROXY='off', GOTOOLCHAIN='local', CGO_ENABLED='0')
    try:
        with tempfile.TemporaryDirectory(prefix='pd-logging92-') as directory:
            record['processes'] = [observe(kind, binary, target, Path(directory), env) for kind, binary in binaries]
            assert record['processes'][0]['responses'] == record['processes'][1]['responses'], record['processes']
            secret = 'TASK092-SECRET-PASSWORD'
            result = subprocess.run([str(binaries[1][1]), '--client-urls=' + url(), '--etcd-endpoints=http://user:' + secret + '@127.0.0.1:1'], capture_output=True, text=True, env=env, timeout=15)
            assert result.returncode == 1 and secret not in result.stdout + result.stderr
            record['sensitive_failure'] = dict(code=result.returncode, stdout=result.stdout, stderr=result.stderr, secret_absent=True)
            bad_config = Path(directory) / 'bad.toml'
            bad_config.write_text('name = ["' + secret + '" invalid]\n')
            result = subprocess.run([str(binaries[1][1]), '--config=' + str(bad_config)], capture_output=True, text=True, env=env, timeout=15)
            assert result.returncode == 0 and secret not in result.stdout + result.stderr
            record['sensitive_config_failure'] = dict(code=result.returncode, stdout=result.stdout, stderr=result.stderr, secret_absent=True)
        record['temporary_directory_cleaned'] = not Path(directory).exists()
    finally:
        record['after'] = config.readonly.snapshot()
        record['readonly_unchanged'] = before == record['after']
        Path(os.environ['PD_CLI_LOGGING_REPORT']).write_text(json.dumps(record, indent=2))
    assert record['readonly_unchanged']
    print('17 admin/log pairs; JSON file logging, hot filtering, SIGTERM cleanup and credential failure passed')


if __name__ == '__main__':
    main()
