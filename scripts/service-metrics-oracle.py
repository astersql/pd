#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Read-only Go/Rust metrics, proxy and Pushgateway comparison on real TCP."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import signal
import socket
import ssl
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def main():
    def interrupted(signum, frame):
        raise KeyboardInterrupt(f'signal {signum}')
    signal.signal(signal.SIGTERM, interrupted)
    ro = module('readonly', 'scripts/pd-compat-readonly.py')
    fixture = module('metric_fixture', 'scripts/service-metrics-fixture.py')
    target = Path(os.environ['CARGO_TARGET_DIR'])
    assert target in [ROOT / f'target/rust-slot-{i}' for i in range(1, 11)]
    assert (ROOT / 'target/rust-slot-locks' / f"slot-{target.name.rsplit('-', 1)[1]}.lock").is_dir()
    binary = ro.binary('PD_COMPAT_GO_BIN')
    before = ro.snapshot()
    env = dict(os.environ, GOFLAGS='-mod=readonly', GOWORK='off', CGO_ENABLED='0', GOPROXY='off', GOTOOLCHAIN='local', CARGO_BUILD_JOBS='4', RUST_TEST_THREADS='4')
    record = dict(go_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), go_build_info=subprocess.check_output(['go', 'version', '-m', str(binary)], env=env).decode(), cases=[], temporary_directories=[])
    assert 'github.com/tikv/pd' in record['go_build_info']
    backend, upstream = fixture.start()
    def address():
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            return f'http://127.0.0.1:{sock.getsockname()[1]}'
    output = target / f'service-metrics-oracle-{time.time_ns()}.json'
    try:
        for case in ['empty', 'http', 'denied', 'https', 'bad-ca', 'bad-host', 'bad-cn']:
            children = []
            item = dict(case=case)
            record['cases'].append(item)
            with tempfile.TemporaryDirectory(prefix='metrics86-oracle-', dir=target) as work:
                work = Path(work)
                record['temporary_directories'].append(str(work))
                url, peer = address(), address()
                config = work / 'pd.toml'
                query_upstream = upstream
                extra_backend = None
                security = ''
                ca = cert = key = None
                if case in ['https', 'bad-ca', 'bad-host', 'bad-cn']:
                    certs = work / 'certs'
                    module('auth_fixture', 'scripts/service-auth-oracle.py').certificates(certs)
                    if case != 'bad-host':
                        (certs / 'extensions').write_text(f'subjectAltName=IP:{fixture.lan_address()}\nextendedKeyUsage=serverAuth,clientAuth\n')
                        subprocess.run(['openssl', 'x509', '-req', '-in', 'server.csr', '-CA', 'ca.pem', '-CAkey', 'ca.key', '-CAcreateserial', '-days', '2', '-extfile', 'extensions', '-out', 'server.pem'], cwd=certs, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
                    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
                    context.load_cert_chain(certs / 'server.pem', certs / 'server.key')
                    context.load_verify_locations(certs / 'ca.pem')
                    context.verify_mode = ssl.CERT_REQUIRED
                    extra_backend, query_upstream = fixture.start(context)
                    ca = certs / ('rogue.pem' if case == 'bad-ca' else 'ca.pem')
                    cert, key = certs / 'server.pem', certs / 'server.key'
                    security = '[security]\n' + '\n'.join(f'{name}={json.dumps(str(path))}' for name, path in [('cacert-path', ca), ('cert-path', cert), ('key-path', key)]) + '\n'
                    if case == 'bad-cn':
                        security += 'cert-allowed-cn=["not-server"]\n'

                text = '' if case == 'empty' else f'[pd-server]\nmetric-storage={json.dumps(query_upstream)}\n'
                if case in ['http', 'https']:
                    text += f'[metric]\njob="oracle/go/{case}"\naddress="http://demo:p%40ss@127.0.0.1:{backend.server_port}"\ninterval="100ms"\n'
                if case == 'denied':
                    text = f'[pd-server]\nmetric-storage="http://127.0.0.1:{backend.server_port}"\n'
                text += security
                config.write_text(text)
                with (work / 'go.log').open('w+') as log:
                    try:
                        command = [str(binary), f'--config={config}', '--name=metrics86', f'--data-dir={work}/data', f'--client-urls={url}', f'--advertise-client-urls={url}', f'--peer-urls={peer}', f'--advertise-peer-urls={peer}', f'--initial-cluster=metrics86={peer}']
                        item['go_command'] = command
                        child = subprocess.Popen(command, env=env, stdout=log, stderr=log, start_new_session=True)
                        children.append(child)
                        deadline = time.monotonic() + 45
                        while True:
                            assert child.poll() is None, f'Go exited {child.returncode}'
                            reply = subprocess.run(['curl', '-fsS', '--max-time', '1', url + '/pd/api/v1/members'], capture_output=True)
                            if reply.returncode == 0 and json.loads(reply.stdout).get('leader', {}).get('member_id'):
                                break
                            assert time.monotonic() < deadline, 'Go readiness timeout'
                            time.sleep(.1)
                        run_env = dict(env, PD_METRICS_GO_URL=url, PD_METRICS_STORAGE_URL='' if case == 'empty' else (f'http://127.0.0.1:{backend.server_port}' if case == 'denied' else query_upstream))
                        if ca:
                            run_env.update(PD_METRICS_CA=str(ca), PD_METRICS_CERT=str(cert), PD_METRICS_KEY=str(key))
                        if case == 'bad-cn':
                            run_env['PD_METRICS_PEER_CNS'] = json.dumps(['not-server'])
                        if case in ['http', 'https']:
                            run_env.update(PD_METRICS_UPSTREAM_URL=query_upstream, PD_METRICS_CONTROL_URL=upstream, PD_METRICS_PUSH_ADDRESS=f'http://demo:p%40ss@127.0.0.1:{backend.server_port}', PD_METRICS_PUSH_JOB=f'oracle/rust/{case}', PD_GO_PUSH_JOB=f'oracle/go/{case}')
                        if case == 'http':
                            run_env['PD_METRICS_PEAK'] = '1'
                        command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-server', '--lib', 'task_086_behavior', '--', '--nocapture']
                        item['rust_command'] = command
                        rust = subprocess.Popen(command, cwd=ROOT, env=run_env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, start_new_session=True)
                        children.append(rust)
                        text, _ = rust.communicate(timeout=180)
                        item.update(rust_log=text, exit_code=rust.returncode)
                        print(text)
                        assert rust.returncode == 0 and '1 passed; 0 failed; 0 ignored' in text
                        observations = [json.loads(line.removeprefix('task086 metrics ')) for line in text.splitlines() if line.startswith('task086 metrics ')]
                        assert len(observations) == 2 and observations[0] == observations[1]
                        inventories = [json.loads(line.removeprefix('task086 inventory ')) for line in text.splitlines() if line.startswith('task086 inventory ')]
                        assert len(inventories) == 2 and inventories[0]['queries'] == inventories[1]['queries'], 'metric queries differ'
                        item.update(observations=observations, inventories_rust_go=inventories, query_routes_equal=True)
                        item['go_only_families'] = sorted(set(map(tuple, inventories[1]['families'])) - set(map(tuple, inventories[0]['families'])))
                        if case == 'http':
                            run_env['PD_METRICS_SNAPSHOTS'] = str(work)
                            rpc_command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-server', '--lib', 'task_086_rpc_measurements', '--', '--nocapture']
                            rpc = subprocess.Popen(rpc_command, cwd=ROOT, env=run_env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, start_new_session=True)
                            children.append(rpc)
                            rpc_text, _ = rpc.communicate(timeout=90)
                            item.update(rpc_command=rpc_command, rpc_log=rpc_text, rpc_exit=rpc.returncode)
                            print(rpc_text)
                            assert rpc.returncode == 0 and '1 passed; 0 failed; 0 ignored' in rpc_text
                            rpc_observations = [json.loads(line.removeprefix('task086 rpc_metrics ')) for line in rpc_text.splitlines() if line.startswith('task086 rpc_metrics ')]
                            assert len(rpc_observations) == 2 and rpc_observations[0] == rpc_observations[1]
                            item['rpc_observations'] = rpc_observations
                            schema = module('metric_schema', 'scripts/service-metrics-schema.py')
                            item['rpc_metric_schemas'] = schema.compare((work / 'rust.txt').read_text(), (work / 'go.txt').read_text())


                    finally:
                        for child in reversed(children):
                            if child.poll() is None:
                                os.killpg(child.pid, signal.SIGTERM)
                                try:
                                    child.wait(timeout=8)
                                except subprocess.TimeoutExpired:
                                    os.killpg(child.pid, signal.SIGKILL)
                                    child.wait(timeout=5)
                        item['children'] = [dict(pid=c.pid, process_group=c.pid, exit_code=c.returncode) for c in children]
                        item['processes_reaped'] = all(c.poll() is not None for c in children)
                        log.flush()
                        log.seek(0)
                        item['go_log'] = log.read()
                        if extra_backend:
                            extra_backend.shutdown()
                            extra_backend.server_close()
            item['temporary_directory_removed'] = not work.exists()
    finally:
        record['temporary_directories_removed'] = all(not Path(p).exists() for p in record['temporary_directories'])
        backend.shutdown()
        backend.server_close()
        record['readonly_unchanged'] = ro.snapshot() == before
        output.write_text(json.dumps(record, ensure_ascii=False, indent=2))
        print(output)
        assert record['readonly_unchanged']


if __name__ == '__main__':
    main()
