#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Task084 read-only Go/Rust mTLS differential and fixed expired fixture."""
import pathlib
import subprocess
import sys

def expired(work):
    work=pathlib.Path(work).resolve()
    # Fixed validity avoids sleeps and wall-clock races; CN is otherwise allowed.
    (work/'expired-index').write_text('')
    (work/'expired-serial').write_text('1000\n')
    (work/'expired.cnf').write_text(f"[ca]\ndefault_ca=issuer\n[issuer]\ndatabase={work}/expired-index\nserial={work}/expired-serial\nnew_certs_dir={work}\ncertificate={work}/ca.pem\nprivate_key={work}/ca.key\ndefault_md=sha256\npolicy=policy\n[policy]\ncommonName=supplied\n")
    subprocess.run(['openssl','ca','-batch','-config',str(work/'expired.cnf'),'-in',str(work/'allowed.csr'),'-out',str(work/'expired.pem'),'-startdate','20000101000000Z','-enddate','20000102000000Z'],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
    (work/'expired.key').write_bytes((work/'allowed.key').read_bytes())

def main():
    import os, json, tempfile, time, socket, signal, hashlib, importlib.util
    sys.dont_write_bytecode = True
    root = pathlib.Path(__file__).resolve().parent.parent
    spec = importlib.util.spec_from_file_location('readonly', root / 'scripts/pd-compat-readonly.py')
    ro = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(ro)
    cert_spec = importlib.util.spec_from_file_location('certificates', root / 'scripts/service-auth-oracle.py')
    cert_module = importlib.util.module_from_spec(cert_spec)
    cert_spec.loader.exec_module(cert_module)
    certificates = cert_module.certificates
    target = pathlib.Path(os.environ['CARGO_TARGET_DIR'])
    assert target in [root / f'target/rust-slot-{n}' for n in range(1, 11)]
    assert (root / f"target/rust-slot-locks/slot-{target.name.rsplit('-', 1)[1]}.lock").is_dir()
    binary = ro.binary('PD_COMPAT_GO_BIN')
    env = dict(os.environ, GOFLAGS='-mod=readonly', GOWORK='off', CGO_ENABLED='0', GOPROXY='off', GOTOOLCHAIN='local')
    before = ro.snapshot()
    children = []
    logs = []
    record = {'go_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'go_build_info': subprocess.check_output(['go', 'version', '-m', str(binary)], env=env).decode()}
    assert 'github.com/tikv/pd' in record['go_build_info']
    record['input_hashes'] = {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in [pathlib.Path(__file__), root / 'pkg/server/service_auth.rs', root / 'pkg/runtime/mtls_listener_test.rs', root / 'Cargo.lock']}
    record['go_before'] = before
    record['transport'] = 'mandatory client authentication; zero/one/multiple CN lists'

    def address(secure=False):
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        return ('https' if secure else 'http') + f'://127.0.0.1:{port}'

    def request(certs, url, path):
        return subprocess.run(['curl', '-sS', '--max-time', '2', '--cacert', str(certs / 'ca.pem'), '--cert', str(certs / 'allowed.pem'), '--key', str(certs / 'allowed.key'), url + '/pd/api/v1/' + path], capture_output=True)

    def interrupted(*_):
        raise KeyboardInterrupt()
    for sig in [signal.SIGTERM, signal.SIGINT]:
        signal.signal(sig, interrupted)
    output = target / f'mtls-listener-oracle-{time.time_ns()}.json'
    try:
        with tempfile.TemporaryDirectory(prefix='mtls84-oracle-', dir=target) as work:
            work = pathlib.Path(work)
            certs = work / 'certs'
            certificates(certs)
            urls = []
            try:
                for i, cns in enumerate([['allowed', 'server'], ['server'], []]):
                    url = address(cns is not None)
                    peer = address()
                    urls.append(url)
                    cfg = work / f'{i}.toml'
                    if cns is not None:
                        cfg.write_text('[security]\n' + f"cacert-path={json.dumps(str(certs / 'ca.pem'))}\ncert-path={json.dumps(str(certs / 'server.pem'))}\nkey-path={json.dumps(str(certs / 'server.key'))}\ncert-allowed-cn={json.dumps(cns)}\n")
                    else:
                        cfg.write_text('')
                    log = open(work / f'{i}.log', 'w+')
                    logs.append(log)
                    child = subprocess.Popen([str(binary), f'--config={cfg}', f'--name=auth{i}', f'--data-dir={work}/data{i}', f'--client-urls={url}', f'--advertise-client-urls={url}', f'--peer-urls={peer}', f'--advertise-peer-urls={peer}', f'--initial-cluster=auth{i}={peer}'], env=env, stdout=log, stderr=log, start_new_session=True)
                    children.append(child)
                    deadline = time.monotonic() + 45
                    while True:
                        assert child.poll() is None, f'Go exited {child.returncode}'
                        r = request(certs, url, 'members')
                        try:
                            ready = json.loads(r.stdout).get('leader', {}).get('member_id')
                        except (ValueError, AttributeError):
                            ready = False
                        if r.returncode == 0 and ready:
                            break
                        assert time.monotonic() < deadline, 'Go readiness timeout'
                        time.sleep(0.1)
                env.update(PD_TLS_CERTIFICATES=str(certs), PD_MTLS_GO_URLS=json.dumps(urls), CARGO_BUILD_JOBS='4', RUST_TEST_THREADS='4')
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib', 'task_084_behavior', '--', '--nocapture']
                record['command'] = command
                rust = subprocess.Popen(command, cwd=root, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, start_new_session=True)
                children.append(rust)
                text, _ = rust.communicate()
                record.update(exit_code=rust.returncode, rust_log=text)
                print(text)
                assert rust.returncode == 0 and '1 passed; 0 failed; 0 ignored' in text
                assert text.count('task084 Go/Rust differential') == 3
                record['go_final_configs'] = [request(certs, url, 'service-middleware/config').stdout.decode() for url in urls]
            finally:
                for child in reversed(children):
                    if child.poll() is None:
                        os.killpg(child.pid, signal.SIGTERM)
                        try:
                            child.wait(timeout=8)
                        except subprocess.TimeoutExpired:
                            os.killpg(child.pid, signal.SIGKILL)
                            child.wait(timeout=5)
                record['processes_reaped'] = all((c.poll() is not None for c in children))
                record['go_logs'] = []
                for log in logs:
                    log.flush()
                    log.seek(0)
                    record['go_logs'].append(log.read())
                    log.close()
        record['temporary_directory_removed'] = not work.exists()
    finally:
        record['readonly_unchanged'] = ro.snapshot() == before
        record['inputs_unchanged'] = all((hashlib.sha256((root / p).read_bytes()).hexdigest() == h for p, h in record['input_hashes'].items()))
        output.write_text(json.dumps(record, ensure_ascii=False, indent=2))
        print(output)
        assert record['readonly_unchanged'] and record['inputs_unchanged']
if __name__ == '__main__':
    if len(sys.argv) == 3 and sys.argv[1] == '--expired':
        expired(sys.argv[2])
    else:
        main()
