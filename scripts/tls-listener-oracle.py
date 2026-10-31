#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Run task083 against an existing Go binary; never build or mutate Go."""
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


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def main():
    def interrupted(signum, frame):
        raise KeyboardInterrupt(f"signal {signum}")
    signal.signal(signal.SIGTERM, interrupted)
    ro = module('readonly', 'scripts/pd-compat-readonly.py')
    certificates = module('auth', 'scripts/service-auth-oracle.py').certificates
    target = Path(os.environ['CARGO_TARGET_DIR'])
    assert target in [ROOT / f'target/rust-slot-{i}' for i in range(1, 11)]
    assert (ROOT / 'target/rust-slot-locks' / f"slot-{target.name.rsplit('-', 1)[1]}.lock").is_dir()
    binary = ro.binary('PD_COMPAT_GO_BIN')
    before = ro.snapshot()
    env = dict(os.environ, GOFLAGS='-mod=readonly', GOWORK='off', CGO_ENABLED='0',
               GOPROXY='off', GOTOOLCHAIN='local', CARGO_BUILD_JOBS='4', RUST_TEST_THREADS='4')
    record = {'go_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'go_build_info': subprocess.check_output(['go', 'version', '-m', str(binary)], env=env).decode()}
    assert 'github.com/tikv/pd' in record['go_build_info']
    children = []
    def address(scheme):
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            return f'{scheme}://127.0.0.1:{sock.getsockname()[1]}'
    output = target / f'tls-listener-oracle-{time.time_ns()}.json'
    try:
        with tempfile.TemporaryDirectory(prefix='tls83-oracle-', dir=target) as work:
            work = Path(work)
            certs = work / 'certs'
            certificates(certs)
            url, peer = address('https'), address('http')
            config = work / 'go.toml'
            config.write_text('[security]\n' + '\n'.join(f'{key}={json.dumps(str(certs / filename))}' for key, filename in [('cacert-path', 'ca.pem'), ('cert-path', 'server.pem'), ('key-path', 'server.key')]) + '\n')
            with (work / 'go.log').open('w+') as log:
                try:
                    command = [str(binary), f'--config={config}', '--name=tls83', f'--data-dir={work}/data', f'--client-urls={url}', f'--advertise-client-urls={url}', f'--peer-urls={peer}', f'--advertise-peer-urls={peer}', f'--initial-cluster=tls83={peer}']
                    record['go_command'] = command
                    child = subprocess.Popen(command, env=env, stdout=log, stderr=log, start_new_session=True)
                    children.append(child)
                    deadline = time.monotonic() + 45
                    while True:
                        assert child.poll() is None, f'Go exited {child.returncode}'
                        reply = subprocess.run(['curl', '-fsS', '--max-time', '1', '--cacert', str(certs / 'ca.pem'), '--cert', str(certs / 'allowed.pem'), '--key', str(certs / 'allowed.key'), url + '/pd/api/v1/members'], capture_output=True)
                        if reply.returncode == 0 and json.loads(reply.stdout).get('leader', {}).get('member_id'):
                            break
                        assert time.monotonic() < deadline, 'Go readiness timeout'
                        time.sleep(.1)
                    env.update(PD_TLS_CERTIFICATES=str(certs), PD_TLS_GO_URL=url)
                    command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib', 'task_083_behavior', '--', '--nocapture']
                    record['rust_command'] = command
                    rust = subprocess.Popen(command, cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, start_new_session=True)
                    children.append(rust)
                    text, _ = rust.communicate()
                    record.update(rust_log=text, exit_code=rust.returncode)
                    print(text)
                    assert rust.returncode == 0 and '1 passed; 0 failed; 0 ignored' in text
                    assert 'task083 Go/Rust same-port' in text
                finally:
                    for child in reversed(children):
                        if child.poll() is None:
                            os.killpg(child.pid, signal.SIGTERM)
                            try:
                                child.wait(timeout=8)
                            except subprocess.TimeoutExpired:
                                os.killpg(child.pid, signal.SIGKILL)
                                child.wait(timeout=5)
                    record['processes_reaped'] = all(c.poll() is not None for c in children)
                    log.flush()
                    log.seek(0)
                    record['go_log'] = log.read()
        record['temporary_directory_removed'] = not work.exists()
    finally:
        record['readonly_unchanged'] = ro.snapshot() == before
        output.write_text(json.dumps(record, ensure_ascii=False, indent=2))
        print(output)
        assert record['readonly_unchanged']


if __name__ == '__main__':
    main()
