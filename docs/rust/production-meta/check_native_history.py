#!/usr/bin/env python3
"""Check native quorum histories on isolated loopback subprocesses."""

import argparse
import base64
from concurrent.futures import ThreadPoolExecutor
import hashlib
import http.client
import json
import os
from pathlib import Path
import random
import signal
import socket
import subprocess
import time


def encoded(value):
    return base64.b64encode(value).decode()


def ports(count):
    listeners = []
    try:
        for _ in range(count):
            listener = socket.socket()
            listener.bind(("127.0.0.1", 0))
            listeners.append(listener)
        return [listener.getsockname()[1] for listener in listeners]
    finally:
        for listener in listeners:
            listener.close()


class Cluster:
    def __init__(self, binary, root):
        self.binary = binary
        self.root = root
        # Hold the six sockets together until all addresses have been reserved.
        addresses = ports(6)
        self.clients, self.peers = addresses[:3], addresses[3:]
        self.children = {}

    def start(self, member, initialize=False):
        environment = dict(os.environ, PD_NATIVE_ID=str(member + 1),
                           PD_NATIVE_CLUSTER="72",
                           PD_NATIVE_DIR=str(self.root / f"node-{member + 1}"),
                           PD_NATIVE_CLIENT=f"127.0.0.1:{self.clients[member]}",
                           PD_NATIVE_PEERS=",".join(f"127.0.0.1:{p}" for p in self.peers))
        environment.pop("PD_NATIVE_INITIALIZE", None)
        if initialize:
            environment["PD_NATIVE_INITIALIZE"] = "1"
        with (self.root / f"node-{member + 1}.log").open("ab") as log:
            self.children[member] = subprocess.Popen(
                [str(self.binary), "--ignored", "--exact", "native_process_test::native_node_child",
                 "--nocapture"], env=environment, stdout=log, stderr=subprocess.STDOUT)

    def stop(self, member, crash=False):
        process = self.children.pop(member)
        process.send_signal(signal.SIGKILL if crash else signal.SIGTERM)
        try:
            code = process.wait(timeout=8)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
            raise
        expected = -signal.SIGKILL if crash else 0
        if code != expected:
            raise RuntimeError(f"node {member + 1} exit {code}, expected {expected}")

    def close(self):
        failures = []
        for member in list(self.children):
            try:
                self.stop(member)
            except Exception as error:
                failures.append(str(error))
        if failures:
            raise RuntimeError("; ".join(failures))

    def call(self, member, route, body=None):
        connection = http.client.HTTPConnection("127.0.0.1", self.clients[member], timeout=3)
        try:
            connection.request("POST", f"/{route}", json.dumps(body),
                               {"content-type": "application/json"})
            response = connection.getresponse()
            if response.status != 200:
                raise RuntimeError(f"{route} unexpected HTTP status {response.status}")
            return json.loads(response.read())
        except (OSError, http.client.RemoteDisconnected):
            return {"error": "Unavailable"}
        finally:
            connection.close()

    def leader(self, excluded=None):
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            for member in tuple(self.children):
                if member == excluded:
                    continue
                status = self.call(member, "status")
                if status.get("role") == "Leader":
                    if "revision" in self.call(member, "read", {"prefix": encoded(b"slot/")}):
                        return member
            time.sleep(0.025)
        raise RuntimeError("quorum failed to elect a readable leader")

    def isolate(self, member, isolate):
        assert self.call(member, "isolate", isolate) == {"isolated": isolate}


def ensure(condition, message):
    if not condition:
        raise AssertionError(message)


def check_history(history, initial, final, cycle):
    """Resolve unknown writes using a unique marker from the final linearizable snapshot."""
    by_key = {bytes(entry["key"]): entry for entry in final["entries"]}
    committed = {}
    timeline = []
    for operation in history:
        if "revision" not in operation:
            ensure(operation.get("error") == "Unavailable", f"unexpected result: {operation}")
        if operation["kind"] == "write":
            token = operation["token"]
            marker = by_key.get(f"slot/marker/{token.split(':')[1]}".encode())
            if marker is not None and bytes(marker["value"]).decode() != token:
                marker = None
            if "revision" in operation:
                ensure(marker is not None, f"acknowledged token lost: {token}")
                ensure(marker["mod_revision"] == operation["revision"], f"revision changed: {token}")
            if marker is not None:
                revision = marker["mod_revision"]
                ensure(revision not in committed, f"duplicate transaction revision: {revision}")
                committed[revision] = token
                # An unknown response does not constrain the completion time.
                timeline.append((operation["start"], operation.get("end") if "revision" in operation else None,
                                 revision, True))
        elif "revision" in operation:
            revision = operation["revision"]
            timeline.append((operation["start"], operation["end"], revision, False))
    ensure(sorted(committed) == list(range(initial["revision"] + 1, final["revision"] + 1)),
           f"revision gap in cycle {cycle}")
    for operation in history:
        if operation["kind"] == "read" and "revision" in operation:
            revision = operation["revision"]
            expected = initial["token"] if revision == initial["revision"] else committed.get(revision)
            ensure(revision == initial["revision"] or revision in committed, f"unexplained read: {operation}")
            ensure(operation["observed"] == ([] if expected is None else [expected] * 2),
                   f"read saw a partial or stale transaction: {operation}")
    for _, end, previous, _ in timeline:
        if end is None:
            continue
        for start, _, current, is_write in timeline:
            if end < start:
                ensure(previous < current if is_write else previous <= current,
                       f"real-time order reversed in cycle {cycle}: {previous} -> {current}")
    last = [bytes(by_key[key]["value"]).decode() for key in [b"slot/data/a", b"slot/data/b"]
            if key in by_key]
    expected = committed.get(final["revision"], initial["token"])
    ensure(last == ([] if expected is None else [expected] * 2), "final atomic pair disagrees with history")
    return {"confirmed": sum(op["kind"] == "write" and "revision" in op for op in history),
            "unknown": sum(op["kind"] == "write" and "revision" not in op for op in history),
            "committed": len(committed), "reads": sum(op["kind"] == "read" and "revision" in op
                                                       for op in history), "tail": expected}


def cycle(cluster, number, rng):
    original = cluster.leader()
    initial_response = cluster.call(original, "read", {"prefix": encoded(b"slot/data/")})
    entries = initial_response["entries"]
    initial = {"revision": initial_response["revision"],
               "token": bytes(entries[0]["value"]).decode() if entries else None}
    history = []
    crash = number % 2 == 0
    # Targets and injection time are reproducible from the seed.
    victim = original if crash or rng.choice([False, True]) else rng.choice(
        [member for member in cluster.children if member != original])
    active = [original]
    operations = list(range(1, 25))
    targets = rng.choices([0, 1, 2, None, None, None, None], k=len(operations))

    def client(ids):
        for index in ids:
            target = targets[index - 1]
            member = active[0] if target is None else target
            token = f"{number}:{index}"
            pairs = [(encoded(k), encoded(token.encode())) for k in
                     [b"slot/data/a", b"slot/data/b", f"slot/marker/{index}".encode()]]
            start = time.monotonic_ns()
            response = cluster.call(member, "write", {"puts": pairs})
            history.append({"kind": "write", "token": token, "member": member + 1,
                            "start": start, "end": time.monotonic_ns(), **response})
            start = time.monotonic_ns()
            response = cluster.call(active[0], "read", {"prefix": encoded(b"slot/data/")})
            entries = response.pop("entries", [])
            history.append({"kind": "read", "start": start, "end": time.monotonic_ns(),
                            "observed": [bytes(item["value"]).decode() for item in entries], **response})

    with ThreadPoolExecutor(max_workers=4) as workers:
        jobs = [workers.submit(client, operations[index::4]) for index in range(4)]
        time.sleep(rng.uniform(0.006, 0.04))
        fault_start = time.monotonic_ns()
        if crash:
            cluster.stop(victim, crash=True)
        else:
            cluster.isolate(victim, True)
        active[0] = cluster.leader(excluded=victim)
        if not crash:
            for route, body in [
                ("read", {"prefix": encoded(b"slot/data/")}),
                ("write", {"puts": [(encoded(b"must-not-write"), encoded(b"bad"))]}),
            ]:
                ensure(cluster.call(victim, route, body) == {"error": "Unavailable"},
                       f"minority accepted {route}")
        for job in jobs:
            job.result(timeout=30)
        if crash:
            cluster.start(victim)
        else:
            cluster.isolate(victim, False)
    final = cluster.call(cluster.leader(), "read", {"prefix": encoded(b"slot/")})
    raw = {"cycle": number, "fault": "sigkill" if crash else "partition", "victim": victim + 1,
           "fault_start": fault_start, "initial": initial, "history": history, "final": final}
    # Preserve the complete failed history as well as its real on-disk cluster.
    with (cluster.root / "histories.jsonl").open("a") as output:
        output.write(json.dumps(raw, separators=(",", ":")) + "\n")
        output.flush()
        os.fsync(output.fileno())
    result = check_history(history, initial, final, number)
    in_flight = sum(op["start"] <= fault_start <= op["end"] for op in history)
    return {**result, "cycle": number, "fault": raw["fault"], "victim": victim + 1,
            "in_flight_at_fault": in_flight,
            "revision": final["revision"]}


def footprint(cluster):
    return {str(member + 1): {
        "bytes": sum(p.stat().st_size for p in (cluster.root / f"node-{member + 1}").iterdir()),
        "rss_kib": int(subprocess.check_output(
            ["ps", "-o", "rss=", "-p", str(child.pid)], text=True).strip()),
    } for member, child in cluster.children.items()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("run", type=Path, help="new isolated evidence directory")
    parser.add_argument("--seed", type=int, default=731091)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--cycles", type=int)
    mode.add_argument("--hours", type=float)
    options = parser.parse_args()
    if (options.hours or options.cycles or 0) <= 0:
        parser.error("the window must be positive")
    binary = options.binary.resolve(strict=True)
    root = options.run.resolve()
    root.mkdir(mode=0o700)
    cluster = Cluster(binary, root)
    start = time.monotonic()
    wall_start = time.time()
    metadata = {"run_id": root.name, "started_unix": wall_start, "seed": options.seed,
                "window_hours": options.hours, "window_cycles": options.cycles,
                "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}
    (root / "run.json").write_text(json.dumps(metadata, indent=2))
    count = 0
    coverage = {"partition": 0, "sigkill": 0}
    try:
        for member in range(3):
            cluster.start(member, initialize=member == 0)
        cluster.leader()
        with (root / "checkpoints.jsonl").open("a") as log:
            while True:
                count += 1
                result = cycle(cluster, count, random.Random(options.seed ^ count))
                if count % 8 == 0:
                    response = cluster.call(cluster.leader(), "compact", result["revision"])
                    ensure(response == {"compacted": result["revision"]}, "compaction failed")
                elapsed = time.monotonic() - start
                result.update(elapsed_seconds=elapsed, wall_elapsed_seconds=time.time() - wall_start,
                              resources=footprint(cluster))
                coverage[result["fault"]] += result["in_flight_at_fault"]
                if options.hours:
                    for sample in result["resources"].values():
                        ensure(sample["bytes"] <= 64 * 1024 * 1024, "native data grew past 64 MiB")
                        ensure(sample["rss_kib"] <= 128 * 1024, "native RSS grew past 128 MiB")
                log.write(json.dumps(result) + "\n")
                log.flush()
                os.fsync(log.fileno())
                if options.cycles and count >= options.cycles:
                    break
                if options.hours and elapsed >= 3600 * options.hours:
                    break
                if options.hours:
                    time.sleep(min(15, max(0, 3600 * options.hours - elapsed)))
    except BaseException as error:
        (root / "failed.txt").write_text(f"{type(error).__name__}: {error}\n")
        raise
    finally:
        cluster.close()
    if options.hours or options.cycles >= 4:
        ensure(all(coverage.values()), f"faults lacked concurrent operations: {coverage}")
    evidence = {file: hashlib.sha256((root / file).read_bytes()).hexdigest()
                for file in ["checkpoints.jsonl", "histories.jsonl"]}
    metadata.update(sha256=evidence, overlap_at_fault=coverage, ended_unix=time.time(), elapsed_seconds=time.monotonic() - start,
                    cycles=count, all_processes_reaped=True)
    (root / "completed.json").write_text(json.dumps(metadata, indent=2))
    print(json.dumps(metadata))


def terminate(_signum, _frame):
    raise KeyboardInterrupt("native run interrupted")


if __name__ == "__main__":
    signal.signal(signal.SIGTERM, terminate)
    main()
