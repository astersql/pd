#!/usr/bin/env python3
"""Validate a complete local native soak report and replay its recorded histories."""

import argparse
import hashlib
from itertools import zip_longest
import json
from pathlib import Path
import statistics

from check_native_history import check_history, ensure


WINDOW_HOURS = 24
MAX_CHECKPOINT_GAP = 90
MIN_CYCLES = 2400


def json_lines(path):
    with path.open() as stream:
        for line in stream:
            yield json.loads(line)


def verify(run):
    started = json.loads((run / "run.json").read_text())
    complete = json.loads((run / "completed.json").read_text())
    ensure(complete["run_id"] == run.name == started["run_id"], "run identity mismatch")
    for field in ["started_unix", "seed", "binary_sha256", "window_hours"]:
        ensure(complete[field] == started[field], f"changed run field: {field}")
    ensure(complete["window_hours"] == WINDOW_HOURS, "incorrect soak window")
    ensure(complete["elapsed_seconds"] >= WINDOW_HOURS * 3600, "short monotonic window")
    ensure(complete["ended_unix"] - complete["started_unix"] >= WINDOW_HOURS * 3600,
           "short real window")
    ensure(complete["all_processes_reaped"], "fixture processes remain")
    ensure(complete["cycles"] >= MIN_CYCLES, "insufficient workload")
    ensure(all(complete["overlap_at_fault"].get(fault, 0) > 0 for fault in ["partition", "sigkill"]),
           "missing concurrent partition or kill")
    for file in ["checkpoints.jsonl", "histories.jsonl"]:
        ensure(hashlib.sha256((run / file).read_bytes()).hexdigest() == complete["sha256"][file],
               f"corrupted evidence: {file}")

    counts = {"partition": 0, "sigkill": 0, "confirmed": 0, "unknown": 0, "reads": 0}
    last = 0
    wall_last = 0
    resource_samples = []
    previous = None
    histories = json_lines(run / "histories.jsonl")
    checkpoints = json_lines(run / "checkpoints.jsonl")
    for index, (checkpoint, history) in enumerate(zip_longest(checkpoints, histories), 1):
        ensure(checkpoint is not None and history is not None, "unmatched history and checkpoint")
        if previous is not None:
            ensure(history["initial"] == previous, "verified revision or value changed between cycles")
        ensure(checkpoint["cycle"] == history["cycle"] == index, "checkpoint sequence changed")
        ensure(checkpoint["fault"] == history["fault"], "checkpoint fault changed")
        ensure(0 < checkpoint["elapsed_seconds"] - last <= MAX_CHECKPOINT_GAP,
               "workload continuity gap")
        last = checkpoint["elapsed_seconds"]
        ensure(0 < checkpoint["wall_elapsed_seconds"] - wall_last <= MAX_CHECKPOINT_GAP,
               "real-time workload continuity gap")
        wall_last = checkpoint["wall_elapsed_seconds"]
        checked = check_history(history["history"], history["initial"], history["final"], index)
        for key, value in checked.items():
            ensure(checkpoint[key] == value, f"checkpoint mismatch: {key}")
        counts[history["fault"]] += 1
        for key in ["confirmed", "unknown", "reads"]:
            counts[key] += checked[key]
        resources = checkpoint["resources"]
        ensure(set(resources) == {"1", "2", "3"}, "incomplete resource sample")
        for values in resources.values():
            ensure(values["bytes"] <= 64 * 1024 * 1024 and values["rss_kib"] <= 128 * 1024,
                   "resource ceiling exceeded")
        previous = {"revision": checkpoint["revision"], "token": checked["tail"]}
        resource_samples.append((max(v["rss_kib"] for v in resources.values()),
                                 max(v["bytes"] for v in resources.values())))
    ensure(len(resource_samples) == complete["cycles"], "incomplete checkpoint stream")
    ensure(min(last, wall_last) >= WINDOW_HOURS * 3600, "last successful checkpoint before end of window")
    ensure(min(counts.values()) > 0, "missing workload or fault outcome")
    # Ignore initial allocator and SQLite warmup. Compare equal-sized windows.
    early = resource_samples[200:400]
    late = resource_samples[-200:]
    growth = [statistics.median(v[i] for v in late) - statistics.median(v[i] for v in early)
              for i in range(2)]
    ensure(growth[0] <= 16 * 1024 and growth[1] <= 16 * 1024 * 1024,
           "sustained RSS or disk growth after warmup")
    return {"run_id": run.name, "elapsed_seconds": last, "cycles": complete["cycles"],
            "counts": counts, "median_growth": {"rss_kib": growth[0], "disk_bytes": growth[1]}}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    options = parser.parse_args()
    print(json.dumps(verify(options.run), indent=2))
