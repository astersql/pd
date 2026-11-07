#!/usr/bin/env python3
"""Calibrate a serial write-back block model using disposable disk images."""

import hashlib
import json
import os
import stat
import tempfile
from pathlib import Path


SECTOR_SIZE = 512
SECTORS = 4
EMPTY = bytes(SECTOR_SIZE)
DATA = b"d" * SECTOR_SIZE
TAIL = b"t" * SECTOR_SIZE


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


class VirtualDisk:
    def __init__(self, image):
        self.disk = image.open("x+b", buffering=0)
        require(stat.S_ISREG(os.fstat(self.disk.fileno()).st_mode), "expected regular file")
        self.disk.truncate(SECTORS * SECTOR_SIZE)
        os.fsync(self.disk.fileno())
        self.cache = {}
        self.events = []

    def persist(self, sector, data):
        self.disk.seek(sector * SECTOR_SIZE)
        require(self.disk.write(data) == SECTOR_SIZE, "short write")

    def write(self, sector, data, *, fua=False):
        require(0 <= sector < SECTORS and len(data) == SECTOR_SIZE, "out of range write")
        if fua:
            self.persist(sector, data)
            os.fsync(self.disk.fileno())
            self.cache.pop(sector, None)
        else:
            self.cache[sector] = data
        self.events.append({"op": "fua" if fua else "write", "sector": sector,
                            "sha256": digest(data)})

    def flush(self):
        for sector, data in self.cache.items():
            self.persist(sector, data)
        os.fsync(self.disk.fileno())
        self.events.append({"op": "flush", "sectors": list(self.cache)})
        self.cache.clear()

    def cut(self):
        self.events.append({"op": "cut", "lost_sectors": list(self.cache)})
        self.cache.clear()
        self.disk.close()


def acknowledged(ack_log, image):
    """Reconcile the separately persisted client log with a fresh disk read."""
    recovered = image.read_bytes()
    missing = []
    for line in ack_log.read_text().splitlines():
        ack = json.loads(line)
        start = ack["sector"] * SECTOR_SIZE
        if digest(recovered[start:start + SECTOR_SIZE]) != ack["sha256"]:
            missing.append(ack["sector"])
    return missing, recovered


def run_case(root, case):
    image = root / f"{case}.img"
    ack_log = root / f"{case}.client.jsonl"
    disk = VirtualDisk(image)
    if case == "fua":
        disk.write(0, TAIL)
    disk.write(0, DATA, fua=case == "fua")
    if case == "flush":
        disk.flush()
    with ack_log.open("x") as log:
        log.write(json.dumps({"sector": 0, "sha256": digest(DATA)}) + "\n")
        log.flush()
        os.fsync(log.fileno())
    disk.events.append({"op": "client_ack", "sector": 0})
    disk.write(1, TAIL)
    disk.cut()

    missing, recovered = acknowledged(ack_log, image)
    expected_loss = [0] if case == "premature_ack" else []
    expected_image = (EMPTY if expected_loss else DATA) + EMPTY * (SECTORS - 1)
    require(missing == expected_loss, f"{case}: incorrect acknowledgement reconciliation")
    require(recovered == expected_image, f"{case}: unexpected recovered sectors")
    return {"case": case, "events": disk.events, "missing_acknowledged_sectors": missing,
            "recovered_image_sha256": digest(recovered)}


def main():
    with tempfile.TemporaryDirectory(prefix="pd-meta-power-") as work:
        root = Path(work)
        require(root.stat().st_mode & 0o077 == 0, "experiment directory is not private")
        results = [run_case(root, case) for case in ("premature_ack", "flush", "fua")]
    print(json.dumps({"sector_size": SECTOR_SIZE, "sector_count": SECTORS,
                      "cases": results}, indent=2))


if __name__ == "__main__":
    main()
