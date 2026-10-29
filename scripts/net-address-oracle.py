#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Read-only existing Go PD endpoint observations, compared with Rust endpoints."""
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

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("contract", ROOT / "scripts/http-contract-oracle.py")
contract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(contract)


def main():
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, contract.interrupted)
    target = Path(os.environ["CARGO_TARGET_DIR"])
    assert target in [ROOT / f"target/rust-slot-{n}" for n in range(1, 11)]
    assert (ROOT / f"target/rust-slot-locks/slot-{target.name.rsplit('-', 1)[1]}.lock").is_dir()
    binary = contract.readonly.binary("PD_COMPAT_GO_BIN")
    info = subprocess.check_output(["go", "version", "-m", str(binary)]).decode()
    assert "\tmod\tgithub.com/tikv/pd\t" in info or "\tdep\tgithub.com/tikv/pd\t" in info
    before = contract.readonly.snapshot()
    record = {"build_info": info, "binary_sha256": contract.hashlib.sha256(binary.read_bytes()).hexdigest(), "cases": []}
    output = target / f"net-address-oracle-{time.time_ns()}.json"
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def request(url):
        with opener.open(url, timeout=2) as response:
            return json.load(response)

    def reserve(ipv6=False):
        sock = socket.socket(socket.AF_INET6 if ipv6 else socket.AF_INET)
        sock.bind(("::1" if ipv6 else "127.0.0.1", 0))
        sock.listen()
        host = "[::1]" if ipv6 else "127.0.0.1"
        return sock, f"http://{host}:{sock.getsockname()[1]}"

    try:
        with tempfile.TemporaryDirectory(prefix="net-address-", dir=target) as work:
            for index in range(6):
                client_socket, client = reserve(index == 1)
                peer_socket, peer = reserve()
                if index >= 2:
                    client = ["127.0.0.1:2379", "http://::1:2379", "http://127.0.0.1:bad", "http://[not-ip]:2379"][index - 2]
                case = {"url": client, "accepted": index < 2}
                record["cases"].append(case)
                log_path = Path(work) / f"go-{index}.log"
                # Existing Go executable cannot inherit listeners; only its oracle
                # setup releases reservations. Rust's owned listener test has no gap.
                client_socket.close()
                peer_socket.close()
                with log_path.open("w") as log:
                    child = subprocess.Popen([str(binary), "--name=compat", f"--data-dir={work}/data-{index}", f"--client-urls={client}", f"--advertise-client-urls={client}", f"--peer-urls={peer}", f"--advertise-peer-urls={peer}", f"--initial-cluster=compat={peer}"], stdout=log, stderr=log, start_new_session=True)
                    try:
                        if index < 2:
                            deadline = time.monotonic() + 40
                            while True:
                                assert child.poll() is None, "Go oracle exited before readiness"
                                try:
                                    members = request(client + "/pd/api/v1/members")
                                    if members.get("leader", {}).get("member_id", 0):
                                        break
                                except (OSError, ValueError):
                                    pass
                                assert time.monotonic() < deadline, "Go readiness timeout"
                                time.sleep(.1)
                            case["members"] = members
                            case["health"] = request(client + "/pd/api/v1/health")
                            assert case["members"] == request(client + "/pd/api/v1/members")
                        else:
                            assert child.wait(timeout=15) != 0
                    finally:
                        contract.stop(child)
                        case["exit"] = child.returncode
                        case["log"] = log_path.read_text()
            output.write_text(json.dumps(record))
            command = ["cargo", "test", "--locked", "-p", "astersql-pd-runtime", "--lib", "task_039_readonly_go_differential", "--", "--ignored", "--nocapture"]
            with tempfile.TemporaryFile(mode="w+") as log:
                child = subprocess.Popen(command, env=dict(os.environ, PD_NET_ADDRESS_ORACLE=str(output)), stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
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
        record["readonly_unchanged"] = contract.readonly.snapshot() == before
        record["work_directory_cleaned"] = not Path(work).exists() if "work" in locals() else True
        output.write_text(json.dumps(record))
        print(f"oracle report: {output}")
        assert record["readonly_unchanged"] and record["work_directory_cleaned"]


if __name__ == "__main__":
    main()
