#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Read-only Go/Rust differential for real bound-member diagnostics."""
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
spec = importlib.util.spec_from_file_location("contract", ROOT / "scripts/http-contract-oracle.py")
contract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(contract)
PATHS = [
    "/pd/api/v1/health", "/pd/api/v1/ping", "/pd/health", "/pd/ping",
    "/pd/api/v2/ready", "/pd/api/v2/ready?verbose",
    "/pd/api/v2/readyz/leader-promotion?verbose",
    "/pd/api/v2/livez?exclude=etcd-serializable-read&verbose",
    "/pd/api/v2/readyz?exclude=etcd-data-corruption&exclude=etcd-linearizable-read&exclude=etcd-non-learner&exclude=etcd-serializable-read&verbose",
    "/pd/api/v2/readyz/not-a-check",
]


def address():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return f"http://127.0.0.1:{listener.getsockname()[1]}"


def request(url):
    try:
        response = urllib.request.urlopen(url, timeout=2)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        return {"status": response.status, "body": response.read().decode(),
                "content_type": response.headers.get("Content-Type", "")}


def main():
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, contract.interrupted)
    target = Path(os.environ["CARGO_TARGET_DIR"])
    assert target in [ROOT / f"target/rust-slot-{n}" for n in range(1, 11)]
    slot = target.name.rsplit("-", 1)[1]
    assert (ROOT / f"target/rust-slot-locks/slot-{slot}.lock").is_dir()
    binary = contract.readonly.binary("PD_COMPAT_GO_BIN")
    before = contract.readonly.snapshot()
    env = dict(os.environ, GOFLAGS="-mod=readonly", GOWORK="off", CGO_ENABLED="0",
               GOPROXY="off", GOTOOLCHAIN="local")
    info = subprocess.check_output(["go", "version", "-m", str(binary)], env=env).decode()
    assert "\tmod\tgithub.com/tikv/pd\t" in info or "\tdep\tgithub.com/tikv/pd\t" in info
    sources = [binary, Path(__file__), ROOT / "pkg/server/http_health.rs",
               ROOT / "pkg/runtime/etcd_diagnostics_test.rs", ROOT / "pkg/runtime/etcd_diagnostics.rs", ROOT / "pkg/runtime/member_role_worker.rs",
               ROOT / "pkg/runtime/node.rs", ROOT / "Cargo.lock"]
    fingerprints = lambda: {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}
    record = {"binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "build_info": info, "responses": {}, "inputs": fingerprints()}
    output = target / f"etcd-diagnostics-oracle-{time.time_ns()}.json"
    try:
        with tempfile.TemporaryDirectory(prefix="health-oracle-", dir=target) as work:
            url, peer = address(), address()
            with open(Path(work) / "go.log", "w") as log:
                child = subprocess.Popen([
                    str(binary), "--name=probe", f"--data-dir={work}/data",
                    f"--client-urls={url}", f"--advertise-client-urls={url}",
                    f"--peer-urls={peer}", f"--advertise-peer-urls={peer}",
                    f"--initial-cluster=probe={peer}",
                ], stdout=log, stderr=log, start_new_session=True, env=env)
                try:
                    deadline = time.monotonic() + 40
                    while True:
                        assert child.poll() is None, "Go process exited during startup"
                        try:
                            members = request(url + "/pd/api/v1/members")
                            if members["status"] == 200 and json.loads(members["body"]).get("leader", {}).get("member_id"):
                                break
                        except (OSError, ValueError):
                            pass
                        assert time.monotonic() < deadline, "Go readiness timeout"
                        time.sleep(.1)
                    report = Path(work) / "go.json"
                    record["go_url"] = url
                    report.write_text(json.dumps(record))
                    command = ["cargo", "test", "--locked", "-p", "astersql-pd-runtime", "--lib",
                               "task_043_readonly_etcd_diagnostics_differential", "--", "--ignored", "--nocapture"]
                    with tempfile.TemporaryFile(mode="w+") as result_log:
                        rust = subprocess.Popen(command, env=dict(env, PD_ETCD_DIAGNOSTIC_ORACLE=str(report)),
                                                stdout=result_log, stderr=subprocess.STDOUT, start_new_session=True)
                        try:
                            code = rust.wait()
                        finally:
                            contract.stop(rust)
                        result_log.seek(0)
                        text = result_log.read()
                    record.update(command=command, rust_exit=code, rust_log=text)
                    print(text)
                    assert code == 0 and "1 passed; 0 failed; 0 ignored" in text
                finally:
                    contract.stop(child)
                    record["go_exit"] = child.returncode
    finally:
        record["readonly_unchanged"] = contract.readonly.snapshot() == before
        record["inputs_unchanged"] = fingerprints() == record["inputs"]
        record["cleaned"] = not Path(work).exists() if "work" in locals() else True
        output.write_text(json.dumps(record, indent=2))
        print(f"oracle evidence: {output}")
        assert record["readonly_unchanged"] and record["inputs_unchanged"] and record["cleaned"]


if __name__ == "__main__":
    main()
