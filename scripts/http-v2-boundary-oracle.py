#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Observe v2 Go contracts and explicit Rust dependency gaps without rewriting Go."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("health", ROOT / "scripts/http-health-oracle.py")
health = importlib.util.module_from_spec(spec)
spec.loader.exec_module(health)


def request(url, method="GET", body=None):
    req = urllib.request.Request(url, data=body, method=method)
    try:
        response = urllib.request.urlopen(req, timeout=5)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        return {"status": response.status, "body": response.read().decode(),
                "content_type": response.headers.get("Content-Type", "")}


def main():
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, health.contract.interrupted)
    target = Path(os.environ["CARGO_TARGET_DIR"])
    assert target in [ROOT / f"target/rust-slot-{n}" for n in range(1, 11)]
    assert (ROOT / f"target/rust-slot-locks/slot-{target.name.rsplit('-', 1)[1]}.lock").is_dir()
    binary = health.contract.readonly.binary("PD_COMPAT_GO_BIN")
    before = health.contract.readonly.snapshot()
    sources = [ROOT / "server/apiv2/handlers" / f"{name}.go" for name in
               ("keyspace", "safe_point", "tso_keyspace_group", "meta_service_group", "microservice", "maintenance")]
    inputs = [binary, Path(__file__), ROOT / "pkg/server/http_v2_boundary.rs",
              ROOT / "pkg/server/http_v2_boundary_test.rs", ROOT / "Cargo.lock", *sources]
    hashes = lambda: {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs}
    record = {"inputs": hashes(), "routes": [], "available": {}}
    env = dict(os.environ, GOFLAGS="-mod=readonly", GOWORK="off", CGO_ENABLED="0", GOPROXY="off", GOTOOLCHAIN="local")
    record["build_info"] = subprocess.check_output(["go", "version", "-m", str(binary)], env=env).decode()
    assert "github.com/tikv/pd" in record["build_info"]
    output = target / f"http-v2-boundary-oracle-{time.time_ns()}.json"
    try:
        with tempfile.TemporaryDirectory(prefix="v2-boundary-oracle-", dir=target) as work:
            url, peer = health.address(), health.address()
            with open(Path(work) / "go.log", "w+") as log:
                child = subprocess.Popen([str(binary), "--name=boundary", f"--data-dir={work}/data",
                    f"--client-urls={url}", f"--advertise-client-urls={url}", f"--peer-urls={peer}",
                    f"--advertise-peer-urls={peer}", f"--initial-cluster=boundary={peer}"],
                    stdout=log, stderr=log, start_new_session=True, env=env)
                try:
                    deadline = time.monotonic() + 40
                    while True:
                        assert child.poll() is None, "Go exited during startup"
                        try:
                            if json.loads(request(url + "/pd/api/v1/members")["body"]).get("leader", {}).get("member_id"):
                                break
                        except (OSError, ValueError):
                            pass
                        assert time.monotonic() < deadline, "Go readiness timeout"
                        time.sleep(.1)
                    for source in sources:
                        text = source.read_text()
                        prefix = re.search(r'router := r.Group\("([^"]+)"\)', text)[1]
                        for method, suffix in re.findall(r'router\.(GET|POST|PUT|PATCH|DELETE)\("([^"]*)"', text):
                            suffix = re.sub(r'[:*][A-Za-z_]+', '1', suffix)
                            path = f"{prefix}/{suffix.lstrip('/')}".rstrip('/')
                            response = request(url + "/pd/api/v2/" + path, method, b"invalid JSON must not reach a domain")
                            record["routes"].append({"method": method, "path": path, "go": response, "compatible": False})
                    for path in ("ready?verbose", "affinity-groups"):
                        record["available"][path] = request(url + "/pd/api/v2/" + path)
                    assert len(record["routes"]) == 30
                finally:
                    health.contract.stop(child)
                    record["go_exit"] = child.returncode
                    record["go_group_clean"] = not health.contract.readonly.group_alive(child.pid)
                    log.seek(0)
                    record["go_log"] = log.read()
            report = Path(work) / "go.json"
            report.write_text(json.dumps(record))
            cmd = ["cargo", "test", "--locked", "-p", "astersql-pd-server", "--lib", "task_078_differential", "--", "--ignored", "--nocapture"]
            with tempfile.TemporaryFile(mode="w+") as log:
                child = subprocess.Popen(cmd, env=dict(env, PD_V2_BOUNDARY_ORACLE=str(report)), stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
                try:
                    code = child.wait()
                finally:
                    health.contract.stop(child)
                log.seek(0)
                record.update(command=cmd, rust_exit=code, rust_log=log.read())
                print(record["rust_log"])
                assert code == 0 and "1 passed; 0 failed; 0 ignored" in record["rust_log"]
    finally:
        record["readonly_unchanged"] = health.contract.readonly.snapshot() == before
        record["inputs_unchanged"] = hashes() == record["inputs"]
        record["cleaned"] = not Path(work).exists() if "work" in locals() else True
        output.write_text(json.dumps(record, indent=2))
        print(output)
        assert record["readonly_unchanged"] and record["inputs_unchanged"] and record["cleaned"]


if __name__ == "__main__":
    main()
