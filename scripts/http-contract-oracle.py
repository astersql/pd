#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Capture existing Go HTTP parser behavior and verify the shared Rust contract.

Requires a claimed Cargo slot and PD_COMPAT_GO_BIN; never builds Go or edits
Go inputs. The task-004 snapshot guard is reused. This verifies shared parsing,
not the member-management implementation owned by task 044.
"""
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
spec = importlib.util.spec_from_file_location("readonly", ROOT / "scripts/pd-compat-readonly.py")
readonly = importlib.util.module_from_spec(spec)
spec.loader.exec_module(readonly)


def stop(child):
    if child.poll() is None:
        os.killpg(child.pid, signal.SIGTERM)
        try:
            child.wait(timeout=10)
        except subprocess.TimeoutExpired:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait()


def interrupted(signum, _frame):
    raise SystemExit(128 + signum)


def main():
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, interrupted)
    target = Path(os.environ["CARGO_TARGET_DIR"])
    assert target in [ROOT / f"target/rust-slot-{n}" for n in range(1, 11)]
    slot = target.name.rsplit("-", 1)[1]
    assert (ROOT / f"target/rust-slot-locks/slot-{slot}.lock").is_dir()
    binary = readonly.binary("PD_COMPAT_GO_BIN")
    info = subprocess.check_output(["go", "version", "-m", str(binary)]).decode()
    assert "\tmod\tgithub.com/tikv/pd\t" in info or "\tdep\tgithub.com/tikv/pd\t" in info
    before = readonly.snapshot()
    record = {"binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "build_info": info}
    inputs = [binary, Path(__file__), ROOT / "pkg/server/http_contract.rs", ROOT / "pkg/server/http_contract_test.rs", ROOT / "pkg/server/http.rs", ROOT / "pkg/server/lib.rs", ROOT / "Cargo.lock"]
    fingerprints = lambda: {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in inputs}
    record["inputs"] = fingerprints()
    output = target / f"http-contract-oracle-{time.time_ns()}.json"
    try:
        with tempfile.TemporaryDirectory(prefix="http-contract-", dir=target) as work:
            def address():
                with socket.socket() as listener:
                    listener.bind(("127.0.0.1", 0))
                    return f"http://127.0.0.1:{listener.getsockname()[1]}"
            url, peer = address(), address()
            with open(Path(work) / "go.log", "w") as log:
                child = subprocess.Popen([
                    str(binary), "--name=compat", f"--data-dir={work}/data",
                    f"--client-urls={url}", f"--advertise-client-urls={url}",
                    f"--peer-urls={peer}", f"--advertise-peer-urls={peer}",
                    f"--initial-cluster=compat={peer}"], stdout=log, stderr=log, start_new_session=True)
                record["go_pid"] = child.pid
                def request(path, body=None, content="application/json", method=None):
                    headers = {} if content is None else {"Content-Type": content}
                    req = urllib.request.Request(url + path, data=None if body is None else body.encode(), headers=headers, method=method)
                    # urllib otherwise supplies application/x-www-form-urlencoded.
                    if content is None:
                        req.add_header("Content-Type", "")
                    try:
                        response = urllib.request.urlopen(req, timeout=5)
                    except urllib.error.HTTPError as error:
                        response = error
                    with response:
                        return {"status": response.status, "headers": dict(response.headers), "body": response.read().decode()}
                try:
                    deadline = time.monotonic() + 40
                    while True:
                        assert child.poll() is None, "Go oracle exited"
                        try:
                            if json.loads(request("/pd/api/v1/members")["body"]).get("leader", {}).get("member_id", 0):
                                break
                        except (OSError, ValueError):
                            pass
                        assert time.monotonic() < deadline, "Go oracle readiness timeout"
                        time.sleep(.1)
                    record["before"] = json.loads(request("/pd/api/v1/members")["body"])
                    record["json"] = []
                    for content in ["application/json", "text/plain", None]:
                        for body in ['null', '{}', '{"unknown":null}', '{"a":1,"a":2}', '', ' ', '{', '{}{}', '[]', '1', 'true', '"x"', '{"a":NaN}', '{"a":1e999}', '{"a":"\\ud800"}']:
                            result = request("/pd/api/v1/members/name/compat", body, content)
                            record["json"].append(dict(input=body, content=content, **result))
                    record["uint"] = []
                    for value in ["+1", "-1", "1_0", "0x10", "18446744073709551616"]:
                        result = request("/pd/api/v1/members/id/" + value, method="DELETE")
                        assert result["status"] == 400
                        record["uint"].append(dict(input=value, **result))
                    record["after"] = json.loads(request("/pd/api/v1/members")["body"])
                    assert record["before"] == record["after"]
                finally:
                    stop(child)
                    record["go_exit"] = child.returncode
                    record["go_log"] = (Path(work) / "go.log").read_text()
            output.write_text(json.dumps(record))
            command = ["cargo", "test", "--locked", "-p", "astersql-pd-server", "--lib", "task_038_readonly_go_differential", "--", "--ignored", "--nocapture"]
            with tempfile.TemporaryFile(mode="w+") as log:
                child = subprocess.Popen(command, cwd=ROOT, env=dict(os.environ, PD_HTTP_CONTRACT_ORACLE=str(output)), stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
                record["cargo_pid"] = child.pid
                try:
                    code = child.wait()
                finally:
                    stop(child)
                log.seek(0)
                text = log.read()
            record.update(command=command, exit=code, rust_log=text)
            print(text)
            assert code == 0 and "1 passed; 0 failed; 0 ignored" in text
    finally:
        record["work_directory_cleaned"] = not Path(work).exists() if "work" in locals() else True
        record["inputs_unchanged"] = fingerprints() == record["inputs"]
        record["readonly_unchanged"] = readonly.snapshot() == before
        output.write_text(json.dumps(record))
        print(f"oracle report: {output}; readonly_unchanged={record['readonly_unchanged']}")
        assert record["readonly_unchanged"] and record["inputs_unchanged"] and record["work_directory_cleaned"]


if __name__ == "__main__":
    main()
