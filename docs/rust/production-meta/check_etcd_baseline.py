#!/usr/bin/env python3
"""Measure the experimental baseline on a disposable three-voter etcd cluster."""

import argparse
import base64
import hashlib
import http.client
import json
import math
import signal
import socket
import subprocess
import tempfile
import time
from contextlib import ExitStack
from pathlib import Path


ETCD_SHA = "ec97cf20f1dc01a4d4b3727784b876a2b5a24afbd05409ea5142ea6dcba55a13"
TRANSACTIONS = 128
PAYLOAD = 256
RUNS = 3


def request(conn, method, body):
    conn.request("POST", f"/v3/{method}", json.dumps(body), {"Content-Type": "application/json"})
    response = conn.getresponse()
    payload = json.loads(response.read())
    if response.status != 200 or "error" in payload:
        raise RuntimeError((method, response.status, payload))
    return payload


def encode(data):
    return base64.b64encode(data).decode("ascii")


def keys(index):
    return [encode(f"/acceptance/{index:04}/{part}".encode()) for part in ("a", "b")]


def value(index):
    return encode(index.to_bytes(4, "big") * (PAYLOAD // 4))


def latency(samples):
    return round(sorted(samples)[math.ceil(len(samples) * 0.99) - 1] * 1e3, 3)


class Cluster:
    def __init__(self, binary, root):
        self.binary, self.root = binary, root
        self.nodes = []
        with ExitStack() as sockets:
            ports = []
            for _ in range(6):
                s = sockets.enter_context(socket.socket())
                s.bind(("127.0.0.1", 0))
                ports.append(s.getsockname()[1])
            self.client, self.peer = ports[:3], ports[3:]
        self.cluster = ",".join(f"n{i}=http://127.0.0.1:{p}" for i, p in enumerate(self.peer))

    def start(self):
        for i, (client, peer) in enumerate(zip(self.client, self.peer)):
            client_url, peer_url = f"http://127.0.0.1:{client}", f"http://127.0.0.1:{peer}"
            with (self.root / f"n{i}.log").open("ab") as log:
                self.nodes.append(subprocess.Popen([
                    str(self.binary), "--name", f"n{i}", "--data-dir", str(self.root / f"n{i}"),
                    "--listen-client-urls", client_url, "--advertise-client-urls", client_url,
                    "--listen-peer-urls", peer_url, "--initial-advertise-peer-urls", peer_url,
                    "--initial-cluster", self.cluster, "--initial-cluster-token", "acceptance17",
                    "--initial-cluster-state", "new", "--log-level", "error",
                ], stdout=log, stderr=log))
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            if any(p.poll() is not None for p in self.nodes):
                raise RuntimeError("etcd node exited before readiness")
            statuses = []
            for port in self.client:
                conn = http.client.HTTPConnection("127.0.0.1", port, timeout=0.5)
                try:
                    statuses.append(request(conn, "maintenance/status", {}))
                except (OSError, http.client.HTTPException):
                    break
                finally:
                    conn.close()
            if len(statuses) == 3:
                leaders = {s.get("leader") for s in statuses}
                if len(leaders) == 1:
                    for status, port in zip(statuses, self.client):
                        if status["header"]["member_id"] in leaders:
                            return http.client.HTTPConnection("127.0.0.1", port, timeout=3)
            time.sleep(0.05)
        raise RuntimeError("etcd election timed out")

    def stop(self):
        nodes, self.nodes = self.nodes, []
        for process in nodes:
            if process.poll() is None:
                process.terminate()
        statuses = [process.wait(timeout=20) for process in nodes]
        if any(status not in (0, -signal.SIGTERM) for status in statuses):
            raise RuntimeError(f"unexpected etcd exit codes: {statuses}")


def verify(conn, index):
    for key in keys(index):
        entries = request(conn, "kv/range", {"key": key})["kvs"]
        if [entry["value"] for entry in entries] != [value(index)]:
            raise RuntimeError(f"unexpected state for transaction {index}")


def run(binary, root):
    root.mkdir()
    cluster = Cluster(binary, root)
    writes, reads = [], []
    try:
        conn = cluster.start()
        try:
            for i in range(TRANSACTIONS):
                start = time.perf_counter()
                result = request(conn, "kv/txn", {"success": [
                    {"request_put": {"key": key, "value": value(i)}} for key in keys(i)
                ]})
                writes.append(time.perf_counter() - start)
                if not result.get("succeeded"):
                    raise RuntimeError(f"transaction {i} did not succeed")
                start = time.perf_counter()
                verify(conn, i)
                reads.append(time.perf_counter() - start)
        finally:
            conn.close()
        cluster.stop()
        start = time.perf_counter()
        conn = cluster.start()
        try:
            verify(conn, TRANSACTIONS - 1)
            reopen = time.perf_counter() - start
            for i in range(TRANSACTIONS):
                verify(conn, i)
        finally:
            conn.close()
    finally:
        cluster.stop()
    return {"write_p99_ms": latency(writes), "read_pair_p99_ms": latency(reads),
            "write_txn_per_second": round(TRANSACTIONS / sum(writes), 3),
            "reopen_ms": round(reopen * 1e3, 3), "recovered_pairs": TRANSACTIONS}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("etcd", type=Path, help="verified etcd 3.5.15 binary")
    args = parser.parse_args()
    binary = args.etcd.resolve(strict=True)
    if hashlib.sha256(binary.read_bytes()).hexdigest() != ETCD_SHA:
        raise RuntimeError("etcd binary does not match the pinned oracle")
    with tempfile.TemporaryDirectory(prefix="pd-meta-etcd-baseline-") as work:
        root = Path(work)
        samples = [run(binary, root / f"run{i}") for i in range(RUNS)]
    print(json.dumps({"voters": 3, "transactions": TRANSACTIONS, "bytes_per_value": PAYLOAD,
                      "etcd_sha256": ETCD_SHA, "samples": samples}, indent=2))


if __name__ == "__main__":
    main()
