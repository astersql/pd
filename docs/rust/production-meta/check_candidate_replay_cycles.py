#!/usr/bin/env python3
"""Exercise repeated leader interruptions and durable replay on private files."""

import argparse
import json
import os
import signal
import tempfile
import time
from pathlib import Path

from check_etcd_baseline import CandidateCluster, keys, request, value, verify


CYCLES = 12
WRITES = 128


def interrupt(cluster, conn):
    index = cluster.client.index(conn.port)
    conn.close()
    process = cluster.nodes[index]
    process.kill()
    if process.wait(timeout=10) >= 0:
        raise RuntimeError("leader was not interrupted")
    return index


def run(cluster):
    started = time.monotonic()
    ack = cluster.root / "acks"
    recoveries = []
    conn = cluster.start()
    try:
        with ack.open("x") as log:
            for cycle in range(CYCLES):
                for i in range(cycle * WRITES, (cycle + 1) * WRITES):
                    response = request(conn, "kv/txn", {"success": [
                        {"request_put": {"key": key, "value": value(i)}} for key in keys(i)
                    ]})
                    if not response.get("succeeded"):
                        raise RuntimeError(f"transaction {i} did not succeed")
                    log.write(f"{i}\n")
                log.flush()
                os.fsync(log.fileno())
                member = interrupt(cluster, conn)
                conn = cluster.wait_leader(offline={member})
                if conn.port == cluster.client[member]:
                    raise RuntimeError("interrupted leader was reported healthy")
                verify(conn, (cycle + 1) * WRITES - 1)
                with (cluster.root / f"n{member}.log").open("ab") as trace:
                    cluster.nodes[member] = cluster.spawn(
                        member, cluster.client[member], cluster.peer[member], trace)
                conn.close()
                conn = cluster.wait_leader()
                recoveries.append({"cycle": cycle + 1, "interrupted_member": member + 1,
                                   "verified_tail": (cycle + 1) * WRITES - 1})
        conn.close()
        cluster.stop()
        conn = cluster.start()
        confirmed = [int(line) for line in ack.read_text().splitlines()]
        if confirmed != list(range(CYCLES * WRITES)):
            raise RuntimeError("unexpected acknowledgement log")
        for index in confirmed:
            verify(conn, index)
    finally:
        conn.close()
    return {"seconds": round(time.monotonic() - started, 3), "recoveries": recoveries,
            "confirmed": len(confirmed), "recovered": len(confirmed)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("candidate", type=Path, help="release storage-kv unit test binary")
    binary = parser.parse_args().candidate.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="pd-meta-replay-cycles-") as work:
        cluster = CandidateCluster(binary, Path(work), replay=True)
        try:
            result = run(cluster)
        finally:
            # An interrupted process is already reaped. Prevent the ordinary
            # termination checker from treating that expected exit as a leak.
            cluster.nodes = [p for p in cluster.nodes if p.poll() != -signal.SIGKILL]
            cluster.stop()
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
