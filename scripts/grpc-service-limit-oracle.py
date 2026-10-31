#!/usr/bin/env python3
"""Run identical tonic/HTTP admission scenarios against real Rust and existing Go PD."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('limit', ROOT/'scripts/http-service-limit-oracle.py')
limit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(limit)
health = limit.health


def main():
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, health.contract.interrupted)
    target = Path(os.environ['CARGO_TARGET_DIR'])
    assert target in [ROOT/f'target/rust-slot-{n}' for n in range(1, 11)]
    assert (ROOT/f'target/rust-slot-locks/slot-{target.name.rsplit("-", 1)[1]}.lock').is_dir()
    binary = health.contract.readonly.binary('PD_COMPAT_GO_BIN')
    before = health.contract.readonly.snapshot()
    sources = [binary, Path(__file__), ROOT/'pkg/server/grpc_service_limit.rs', ROOT/'pkg/server/grpc_service_limit_test.rs', ROOT/'pkg/server/http_service_limit.rs', ROOT/'pkg/server/lib.rs', ROOT/'Cargo.lock']
    hashes = lambda: {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}
    inputs = hashes()
    env = dict(os.environ, GOFLAGS='-mod=readonly', GOWORK='off', CGO_ENABLED='0', GOPROXY='off', GOTOOLCHAIN='local')
    record = {'inputs': inputs, 'build_info': subprocess.check_output(['go', 'version', '-m', str(binary)], env=env).decode()}
    assert 'github.com/tikv/pd' in record['build_info']
    output = target/f'grpc-service-limit-oracle-{time.time_ns()}.json'
    try:
        with tempfile.TemporaryDirectory(prefix='grpc-limit-', dir=target) as work:
            url, peer = health.address(), health.address()
            with open(Path(work)/'go.log', 'w+') as log:
                child = subprocess.Popen([str(binary), '--name=limit', f'--data-dir={work}/data', f'--client-urls={url}', f'--advertise-client-urls={url}', f'--peer-urls={peer}', f'--advertise-peer-urls={peer}', f'--initial-cluster=limit={peer}'], env=env, stdout=log, stderr=log, start_new_session=True)
                try:
                    deadline = time.monotonic()+40
                    while True:
                        assert child.poll() is None, 'Go exited during startup'
                        try:
                            members = limit.request(url, 'GET', 'members')[1]
                            if isinstance(members, dict) and members.get('leader', {}).get('member_id'):
                                break
                        except (OSError, ValueError):
                            pass
                        assert time.monotonic() < deadline, 'Go readiness timeout'
                        time.sleep(.1)
                    command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-server', '--lib', 'task_080_wire_and_cancellation', '--', '--nocapture']
                    with tempfile.TemporaryFile(mode='w+') as rust_log:
                        rust = subprocess.Popen(command, env=dict(env, PD_GRPC_LIMIT_GO_URL=url), stdout=rust_log, stderr=subprocess.STDOUT, start_new_session=True)
                        try:
                            code = rust.wait()
                        finally:
                            health.contract.stop(rust)
                        rust_log.seek(0)
                        text = rust_log.read()
                    record.update(command=command, rust_exit=code, rust_log=text)
                    print(text)
                    assert code == 0 and '1 passed; 0 failed; 0 ignored' in text
                    record['go_config'] = limit.request(url, 'GET', 'service-middleware/config')
                finally:
                    health.contract.stop(child)
                    record['go_group_clean'] = not health.contract.readonly.group_alive(child.pid)
                    log.seek(0)
                    record['go_log'] = log.read()
                restarted = subprocess.Popen(child.args, env=env, stdout=log, stderr=log, start_new_session=True)
                try:
                    deadline = time.monotonic()+40
                    while True:
                        assert restarted.poll() is None, 'Go exited during recovery'
                        try:
                            members = limit.request(url, 'GET', 'members')[1]
                            if isinstance(members, dict) and members.get('leader', {}).get('member_id'):
                                break
                        except (OSError, ValueError):
                            pass
                        assert time.monotonic() < deadline, 'Go recovery timeout'
                        time.sleep(.1)
                    record['go_recovered_config'] = limit.request(url, 'GET', 'service-middleware/config')
                    assert record['go_recovered_config'] == record['go_config']
                finally:
                    health.contract.stop(restarted)
                    record['go_recovery_group_clean'] = not health.contract.readonly.group_alive(restarted.pid)
                    log.seek(0)
                    record['go_log'] = log.read()
    finally:
        record['readonly_unchanged'] = health.contract.readonly.snapshot() == before
        record['inputs_unchanged'] = hashes() == inputs
        record['cleaned'] = not Path(work).exists() if 'work' in locals() else True
        output.write_text(json.dumps(record, indent=2))
        print(f'oracle evidence: {output}')
        assert record['readonly_unchanged'] and record['inputs_unchanged'] and record['cleaned']


if __name__ == '__main__':
    main()
