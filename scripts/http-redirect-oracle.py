#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Compare forwarding with a real three-member Go cluster using existing binaries."""
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
import urllib.request
import urllib.error
import hashlib
sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('contract', ROOT / 'scripts/http-contract-oracle.py')
contract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(contract)


def main():
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, contract.interrupted)
    target = Path(os.environ['CARGO_TARGET_DIR'])
    assert target in [ROOT / f'target/rust-slot-{n}' for n in range(1, 11)]
    assert (ROOT / f'target/rust-slot-locks/slot-{target.name.rsplit("-", 1)[1]}.lock').is_dir()
    binary = contract.readonly.binary('PD_COMPAT_GO_BIN')
    before = contract.readonly.snapshot()
    record = {'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'build_info': subprocess.check_output(['go', 'version', '-m', str(binary)]).decode()}
    assert '\tmod\tgithub.com/tikv/pd\t' in record['build_info'] or '\tdep\tgithub.com/tikv/pd\t' in record['build_info']
    inputs = [Path(__file__), ROOT / 'scripts/http-contract-oracle.py', ROOT / 'scripts/pd-compat-readonly.py', ROOT / 'pkg/server/http_redirect.rs', ROOT / 'pkg/server/http_redirect_test.rs', ROOT / 'pkg/server/lib.rs', ROOT / 'Cargo.lock', binary]
    fingerprints = lambda: {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in inputs}
    record['inputs'] = fingerprints()
    output = target / f'http-redirect-oracle-{time.time_ns()}.json'
    children = []
    logs = []
    def address():
        with socket.socket() as s:
            s.bind(('127.0.0.1', 0))
            return f'http://127.0.0.1:{s.getsockname()[1]}'
    def request(url, path):
        with urllib.request.urlopen(url+path, timeout=5) as response:
            return json.load(response)
    try:
        with tempfile.TemporaryDirectory(prefix='http-redirect-', dir=target) as work:
            urls = [address() for _ in range(3)]
            peers = [address() for _ in range(3)]
            cluster = ','.join(f'go-{i}={peers[i]}' for i in range(3))
            try:
                for i in range(3):
                    log = open(Path(work)/f'{i}.log', 'w'); logs.append(log)
                    children.append(subprocess.Popen([str(binary), f'--name=go-{i}', f'--data-dir={work}/data-{i}', f'--client-urls={urls[i]}', f'--advertise-client-urls={urls[i]}', f'--peer-urls={peers[i]}', f'--advertise-peer-urls={peers[i]}', f'--initial-cluster={cluster}'], stdout=log, stderr=log, start_new_session=True))
                deadline = time.monotonic()+45
                while True:
                    assert all(c.poll() is None for c in children)
                    try:
                        members = request(urls[0], '/pd/api/v1/members')
                        if members.get('leader', {}).get('member_id') and len(members['members']) == 3:
                            break
                    except (OSError, ValueError):
                        pass
                    assert time.monotonic() < deadline, 'Go cluster readiness timeout'
                    time.sleep(.1)
                leader = members['leader']['client_urls'][0]
                follower = next(url for url in urls if url != leader)
                record.update(leader=leader, follower=follower, before=members)
                output.write_text(json.dumps(record))
                command = ['cargo','test','--locked','-p','astersql-pd-server','--lib','task_041_readonly_go_differential','--','--ignored','--nocapture']
                with tempfile.TemporaryFile(mode='w+') as log:
                    child = subprocess.Popen(command, env=dict(os.environ, PD_HTTP_REDIRECT_ORACLE=str(output)), stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
                    try: code = child.wait()
                    finally: contract.stop(child)
                    log.seek(0); text=log.read()
                record.update(command=command, exit=code, rust_log=text)
                print(text)
                record['after'] = request(leader, '/pd/api/v1/members')
                assert record['after']['leader']['leader_priority'] == 17
                assert code == 0 and '1 passed; 0 failed; 0 ignored' in text
            finally:
                for child in children: contract.stop(child)
                for log in logs: log.close()
                record['go_exits'] = [c.returncode for c in children]
                record['go_logs'] = [(Path(work)/f'{i}.log').read_text() for i in range(len(logs))]
    finally:
        record['readonly_unchanged'] = contract.readonly.snapshot() == before
        record['inputs_unchanged'] = fingerprints() == record['inputs']
        record['cleaned'] = not Path(work).exists() if 'work' in locals() else True
        output.write_text(json.dumps(record))
        print(f'oracle report: {output}')
        assert record['readonly_unchanged'] and record['cleaned'] and record['inputs_unchanged']


if __name__ == '__main__':
    main()
