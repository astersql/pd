#!/usr/bin/env python3
"""Measure the experimental baseline on a disposable three-voter etcd cluster."""

import argparse
import base64
import functools
import gzip
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
        return self.wait_leader()

    def wait_leader(self, offline=()):
        active = [(i, port) for i, port in enumerate(self.client) if i not in offline]
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            if any(self.nodes[i].poll() is not None for i, _ in active):
                detail = [(i, p.returncode, (self.root / f"n{i}.log").read_text()[-2000:])
                          for i, p in enumerate(self.nodes) if i not in offline and p.poll() is not None]
                raise RuntimeError(f"node exited before readiness: {detail}")
            statuses = []
            for _, port in active:
                conn = http.client.HTTPConnection("127.0.0.1", port, timeout=0.5)
                try:
                    statuses.append(request(conn, "maintenance/status", {}))
                except (OSError, http.client.HTTPException):
                    break
                finally:
                    conn.close()
            if len(statuses) == len(active):
                leaders = {s.get("leader") for s in statuses}
                if len(leaders) == 1:
                    for status, (_, port) in zip(statuses, active):
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
    def __init__(self, binary, root, profile=False, wal_pages=None, replay=False):
        super().__init__(binary, root)
        self.profile = profile
        self.wal_pages = wal_pages
        self.replay = replay

    def spawn(self, i, client, _peer, log):
        db = self.root / f"node{i}.db"
        env = dict(os.environ, PD_CANDIDATE_ID=str(i + 1), PD_CANDIDATE_DB=str(db),
                   PD_CANDIDATE_CLIENT=f"127.0.0.1:{client}",
                   PD_CANDIDATE_PEERS=",".join(f"127.0.0.1:{port}" for port in self.peer))
        env.pop("PD_CANDIDATE_INITIALIZE", None)
        if not db.exists():
            env["PD_CANDIDATE_INITIALIZE"] = "1"
        env.pop("PD_CANDIDATE_PROFILE", None)
        env.pop("PD_CANDIDATE_WAL_PAGES", None)
        env.pop("PD_CANDIDATE_REPLAY", None)
        if self.profile:
            env["PD_CANDIDATE_PROFILE"] = "1"
        if self.wal_pages is not None:
            env["PD_CANDIDATE_WAL_PAGES"] = str(self.wal_pages)
        if self.replay:
            env["PD_CANDIDATE_REPLAY"] = "1"
        return subprocess.Popen([
            str(self.binary), "--ignored", "--exact", "candidate_bench_test::candidate_bench_child",
            "--nocapture",
        ], env=env, stdout=log, stderr=log)

    def trace(self):
        events = []
        for i in range(len(self.client)):
            for line in (self.root / f"n{i}.log").read_text().splitlines():
                if line.startswith("PD_PROFILE "):
                    _, phase, detail, end, elapsed = line.split()
                    events.append({"node": i + 1, "phase": phase, "detail": detail,
                                   "end_us": int(end), "elapsed_us": int(elapsed)})
        return sorted(events, key=lambda e: e["end_us"])


class NativeCluster(Cluster):
    def spawn(self, i, client, _peer, log):
        directory = self.root / f"node-{i + 1}"
        env = dict(os.environ, PD_NATIVE_ID=str(i + 1), PD_NATIVE_CLUSTER="72",
                   PD_NATIVE_DIR=str(directory), PD_NATIVE_CLIENT=f"127.0.0.1:{client}",
                   PD_NATIVE_PEERS=",".join(f"127.0.0.1:{port}" for port in self.peer))
        env.pop("PD_NATIVE_INITIALIZE", None)
        if not directory.exists():
            env["PD_NATIVE_INITIALIZE"] = "1"
        return subprocess.Popen([
            str(self.binary), "--ignored", "--exact", "native_process_test::native_node_child",
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
    metrics = {"write_p99_ms": latency(writes), "read_pair_p99_ms": latency(reads),
               "write_txn_per_second": round(TRANSACTIONS / sum(writes), 3),
               "reopen_ms": round(reopen * 1e3, 3), "recovered_pairs": TRANSACTIONS}
    if isinstance(cluster, CandidateCluster) and cluster.profile:
        metrics["profile"] = cluster.trace()
    return metrics


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("etcd", type=Path, help="verified etcd 3.5.15 binary")
    variants = parser.add_mutually_exclusive_group()
    variants.add_argument("--candidate", type=Path, help="release storage-kv unit test binary")
    variants.add_argument("--native", type=Path, help="release storage-kv native fixture")
    parser.add_argument("--profile-output", type=Path, help="store candidate trace as gzip JSON")
    parser.add_argument("--wal-pages", type=int, help="override auto-checkpoint only for a candidate diagnostic run")
    parser.add_argument("--replay", action="store_true", help="derive candidate state from the durable quorum log")
    args = parser.parse_args()
    binary = args.etcd.resolve(strict=True)
    if hashlib.sha256(binary.read_bytes()).hexdigest() != ETCD_SHA:
        raise RuntimeError("etcd binary does not match the pinned oracle")
    if args.candidate or args.native:
        binary = (args.candidate or args.native).resolve(strict=True)
    if not args.candidate and (args.profile_output or args.wal_pages is not None or args.replay):
        parser.error("candidate diagnostics require --candidate")
    cluster_type = functools.partial(CandidateCluster, profile=bool(args.profile_output),
                                     wal_pages=args.wal_pages, replay=args.replay) if args.candidate else NativeCluster if args.native else Cluster
    with tempfile.TemporaryDirectory(prefix="pd-meta-etcd-baseline-") as work:
        root = Path(work)
        samples = [run(binary, root / f"run{i}", cluster_type) for i in range(RUNS)]
    if args.profile_output:
        traces = [sample.pop("profile") for sample in samples]
        args.profile_output.write_bytes(gzip.compress(json.dumps(traces).encode(), mtime=0))
    report = {"voters": 3, "transactions": TRANSACTIONS, "bytes_per_value": PAYLOAD,
              "etcd_sha256": ETCD_SHA, "samples": samples}
    if args.candidate or args.native:
        baseline = json.loads(Path(__file__).with_name("etcd-baseline.json").read_text())
        assert all(baseline[key] == report[key] for key in (
            "voters", "transactions", "bytes_per_value", "etcd_sha256"))
        metrics = {"write_p99_ms": max, "read_pair_p99_ms": max,
                   "write_txn_per_second": min, "reopen_ms": max, "recovered_pairs": min}
        thresholds = {key: extremum(s[key] for s in baseline["samples"])
                      for key, extremum in metrics.items()}
        report["candidate_sha256"] = hashlib.sha256(binary.read_bytes()).hexdigest()
        report["candidate_mode"] = "native" if args.native else "replay" if args.replay else "materialized"
        report["diagnostic"] = bool(args.profile_output) or args.wal_pages is not None
        report["frozen_thresholds"] = thresholds
        report["meets_thresholds"] = {
            key: all(extremum((s[key], thresholds[key])) == thresholds[key] for s in samples)
            for key, extremum in metrics.items()
        }
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
