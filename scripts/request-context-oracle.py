#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Observe Go request metadata through existing audit logs, without Go writes.

Requires an existing PD_COMPAT_GO_BIN and an owned Cargo shared slot. The Rust
comparison covers the shared HTTP metadata parser, not member administration.
"""
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
import hashlib

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("contract", ROOT / "scripts/http-contract-oracle.py")
contract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(contract)
readonly = contract.readonly


def main():
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, contract.interrupted)
    target = Path(os.environ["CARGO_TARGET_DIR"])
    assert target in [ROOT / f"target/rust-slot-{n}" for n in range(1, 11)]
    assert (ROOT / f"target/rust-slot-locks/slot-{target.name.rsplit('-', 1)[1]}.lock").is_dir()
    binary = readonly.binary("PD_COMPAT_GO_BIN")
    info = subprocess.check_output(["go", "version", "-m", str(binary)]).decode()
    assert "\tmod\tgithub.com/tikv/pd\t" in info or "\tdep\tgithub.com/tikv/pd\t" in info
    before = readonly.snapshot()
    record = {"binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "build_info": info, "cases": []}
    inputs = [binary, Path(__file__), ROOT / "scripts/http-contract-oracle.py", ROOT / "scripts/pd-compat-readonly.py", ROOT / "pkg/server/request_context.rs", ROOT / "pkg/server/request_context_test.rs", ROOT / "pkg/server/lib.rs", ROOT / "Cargo.lock"]
    fingerprints = lambda: {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in inputs}
    record["inputs"] = fingerprints()
    output = target / f"request-context-oracle-{time.time_ns()}.json"
    try:
        with tempfile.TemporaryDirectory(prefix="request-context-", dir=target) as work:
            def address():
                with socket.socket() as listener:
                    listener.bind(("127.0.0.1", 0))
                    return f"http://127.0.0.1:{listener.getsockname()[1]}"
            url, peer = address(), address()
            with open(Path(work) / "go.log", "w") as log:
                child = subprocess.Popen([str(binary), "--name=compat", f"--data-dir={work}/data", f"--client-urls={url}", f"--advertise-client-urls={url}", f"--peer-urls={peer}", f"--advertise-peer-urls={peer}", f"--initial-cluster=compat={peer}"], stdout=log, stderr=log, start_new_session=True)
                record["go_pid"] = child.pid
                def request(path, headers=None, data=None):
                    req = urllib.request.Request(url + path, headers=headers or {}, data=data)
                    try:
                        response = urllib.request.urlopen(req, timeout=5)
                    except urllib.error.HTTPError as error:
                        response = error
                    with response:
                        return dict(status=response.status, body=response.read().decode())
                try:
                    deadline = time.monotonic() + 40
                    while True:
                        assert child.poll() is None, "Go oracle exited"
                        try:
                            if json.loads(request("/pd/api/v1/members")["body"]).get("leader", {}).get("member_id", 0):
                                break
                        except (OSError, ValueError):
                            pass
                        assert time.monotonic() < deadline, "readiness timeout"
                        time.sleep(.1)
                    record["before"] = request("/pd/api/v1/members")
                    cases = [
                        {},
                        {"X-Forwarded-For": "203.0.113.1, 10.0.0.1", "X-Forwarded-Port": "123, 456", "X-Real-IP": "ignored"},
                        {"X-Forwarded-For": " , 203.0.113.1", "X-Real-IP": "198.51.100.1"},
                        {"X-Forwarded-For": "[2001:db8::1]:abc", "X-Forwarded-Port": "123"},
                        {"X-Forwarded-For": "2001:db8::1", "X-Forwarded-Port": "123"},
                        {"X-Forwarded-For": "malformed:x:y", "X-Forwarded-Port": "123"},
                        {"X-Real-IP": "host:"},
                    ]
                    for index, headers in enumerate(cases):
                        headers["X-Caller-ID" if index % 2 else "component"] = f"task040-{index}"
                        result = request("/pd/api/v1/members/name/compat", headers, b"{")
                        assert result["status"] == 400
                        record["cases"].append(dict(headers=headers, **result))
                    record["health"] = request("/pd/api/v1/health", {"X-Caller-ID": "task040-health"})
                    assert record["health"]["status"] == 200
                    record["metrics"] = request("/metrics")["body"]
                    assert any('GetHealthStatus' in line and 'task040-health' in line for line in record["metrics"].splitlines())
                    record["after"] = request("/pd/api/v1/members")
                    assert record["before"] == record["after"]
                finally:
                    contract.stop(child)
                    record["go_exit"] = child.returncode
                    record["go_log"] = (Path(work) / "go.log").read_text()
            for index, case in enumerate(record["cases"]):
                line = next(line for line in record["go_log"].splitlines() if 'audit log' in line and f'CallerID:task040-{index},' in line)
                match = re.search(r'\[service-info=("(?:\\.|[^"])*")\]', line)
                case["info"] = json.loads(match.group(1))
            output.write_text(json.dumps(record))
            command = ["cargo", "test", "--locked", "-p", "astersql-pd-server", "--lib", "task_040_readonly_go_differential", "--", "--ignored", "--nocapture"]
            with tempfile.TemporaryFile(mode="w+") as log:
                child = subprocess.Popen(command, env=dict(os.environ, PD_REQUEST_CONTEXT_ORACLE=str(output)), stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
                try:
                    code = child.wait()
                finally:
                    contract.stop(child)
                log.seek(0)
                text = log.read()
            record.update(command=command, exit=code, rust_log=text)
            print(text)
            assert code == 0 and "1 passed; 0 failed; 0 ignored" in text
    finally:
        record["work_directory_cleaned"] = not Path(work).exists() if "work" in locals() else True
        record["readonly_unchanged"] = readonly.snapshot() == before
        record["inputs_unchanged"] = fingerprints() == record["inputs"]
        output.write_text(json.dumps(record))
        print(f"oracle report: {output}")
        assert record["readonly_unchanged"] and record["work_directory_cleaned"] and record["inputs_unchanged"]


if __name__ == "__main__":
    main()
