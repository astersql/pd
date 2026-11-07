#!/usr/bin/env python3
"""Measure the experimental baseline on a disposable three-voter etcd cluster."""

import argparse
import base64
import hashlib
import http.client
import json
import math
import os
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

    def spawn(self, i, client, peer, log):
        client_url, peer_url = f"http://127.0.0.1:{client}", f"http://127.0.0.1:{peer}"
        return subprocess.Popen([
            str(self.binary), "--name", f"n{i}", "--data-dir", str(self.root / f"n{i}"),
            "--listen-client-urls", client_url, "--advertise-client-urls", client_url,
            "--listen-peer-urls", peer_url, "--initial-advertise-peer-urls", peer_url,
            "--initial-cluster", self.cluster, "--initial-cluster-token", "acceptance17",
            "--initial-cluster-state", "new", "--log-level", "error",
        ], stdout=log, stderr=log)

    def start(self):
        for i, (client, peer) in enumerate(zip(self.client, self.peer)):
            with (self.root / f"n{i}.log").open("ab") as log:
                self.nodes.append(self.spawn(i, client, peer, log))
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            if any(p.poll() is not None for p in self.nodes):
                detail = [(i, p.returncode, (self.root / f"n{i}.log").read_text()[-2000:])
                          for i, p in enumerate(self.nodes) if p.poll() is not None]
                raise RuntimeError(f"node exited before readiness: {detail}")
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
        raise RuntimeError("cluster election timed out")

    def stop(self):
        nodes, self.nodes = self.nodes, []
        for process in nodes:
            if process.poll() is None:
                process.terminate()
        statuses = []
        for process in nodes:
            try:
                statuses.append(process.wait(timeout=20))
            except subprocess.TimeoutExpired:
                process.kill()
                statuses.append(process.wait())
        if any(status not in (0, -signal.SIGTERM) for status in statuses):
            raise RuntimeError(f"unexpected node exit codes: {statuses}")


class CandidateCluster(Cluster):
    def spawn(self, i, client, _peer, log):
        db = self.root / f"node{i}.db"
        env = dict(os.environ, PD_CANDIDATE_ID=str(i + 1), PD_CANDIDATE_DB=str(db),
                   PD_CANDIDATE_CLIENT=f"127.0.0.1:{client}",
                   PD_CANDIDATE_PEERS=",".join(f"127.0.0.1:{port}" for port in self.peer))
        env.pop("PD_CANDIDATE_INITIALIZE", None)
        if not db.exists():
            env["PD_CANDIDATE_INITIALIZE"] = "1"
        return subprocess.Popen([
            str(self.binary), "--ignored", "--exact", "candidate_bench_test::candidate_bench_child",
            "--nocapture",
        ], env=env, stdout=log, stderr=log)


def verify(conn, index):
    for key in keys(index):
        entries = request(conn, "kv/range", {"key": key})["kvs"]
        if [entry["value"] for entry in entries] != [value(index)]:
            raise RuntimeError(f"unexpected state for transaction {index}")


def run(binary, root, cluster_type=Cluster):
    root.mkdir()
    cluster = cluster_type(binary, root)
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
    parser.add_argument("--candidate", type=Path, help="release storage-kv unit test binary")
    args = parser.parse_args()
    binary = args.etcd.resolve(strict=True)
    if hashlib.sha256(binary.read_bytes()).hexdigest() != ETCD_SHA:
        raise RuntimeError("etcd binary does not match the pinned oracle")
    if args.candidate:
        binary = args.candidate.resolve(strict=True)
    cluster_type = CandidateCluster if args.candidate else Cluster
    with tempfile.TemporaryDirectory(prefix="pd-meta-etcd-baseline-") as work:
        root = Path(work)
        samples = [run(binary, root / f"run{i}", cluster_type) for i in range(RUNS)]
    report = {"voters": 3, "transactions": TRANSACTIONS, "bytes_per_value": PAYLOAD,
              "etcd_sha256": ETCD_SHA, "samples": samples}
    if args.candidate:
        baseline = json.loads(Path(__file__).with_name("etcd-baseline.json").read_text())
        assert all(baseline[key] == report[key] for key in (
            "voters", "transactions", "bytes_per_value", "etcd_sha256"))
        metrics = {"write_p99_ms": max, "read_pair_p99_ms": max,
                   "write_txn_per_second": min, "reopen_ms": max, "recovered_pairs": min}
        thresholds = {key: extremum(s[key] for s in baseline["samples"])
                      for key, extremum in metrics.items()}
        report["candidate_sha256"] = hashlib.sha256(binary.read_bytes()).hexdigest()
        report["frozen_thresholds"] = thresholds
        report["meets_thresholds"] = {
            key: all(extremum((s[key], thresholds[key])) == thresholds[key] for s in samples)
            for key, extremum in metrics.items()
        }
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
