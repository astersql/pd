#!/usr/bin/env python3
"""Exercise SQLite's real WAL I/O with disposable, synced-only file images."""

import json
import os
import shutil
import tempfile
from contextlib import closing
from pathlib import Path

import apsw

from check_power_model import digest, require


FILES = {"meta.db", "meta.db-journal", "meta.db-wal"}
MAX_IMAGE = 1024 * 1024
SOURCE_IDS = {
    "3.50.2": "2025-06-28 14:00:48 2af157d77fb1304a74176eaee7fbc7c7e932d946bf25325e9c26c91db19e3079",
    "3.51.3": "2026-03-13 10:38:09 737ae4a34738ffa0c3ff7f9bb18df914dd1cad163f28fd6b6e114a344fe6d618",
}


class CachedFile(apsw.VFSFile):
    def __init__(self, device, name, flags):
        self.device = device
        self.file = device.checked_path(name)
        super().__init__(device.base, name, flags)
        self.live = bytearray(self.file.read_bytes())
        self.pending = []

    def xRead(self, amount, offset):
        return bytes(self.live[offset:offset + amount])

    def xFileSize(self):
        return len(self.live)

    def xDeviceCharacteristics(self):
        return 0

    def xSectorSize(self):
        return 512

    def xFileControl(self, op, pointer):
        # Prevent allocation hints and other opaque operations from bypassing the cache.
        return False

    def xWrite(self, data, offset):
        end = offset + len(data)
        require(0 <= offset <= end <= MAX_IMAGE, "out of range write")
        if end > len(self.live):
            self.live.extend(bytes(end - len(self.live)))
        self.live[offset:end] = data
        self.pending.append((offset, bytes(data), self.device.test_started))
        self.device.trace.append({"op": "write", "file": self.file.name, "offset": offset,
                                  "bytes": len(data), "sha256": digest(data)})

    def xTruncate(self, size):
        require(0 <= size <= MAX_IMAGE, "out of range truncate")
        if size > len(self.live):
            self.live.extend(bytes(size - len(self.live)))
        del self.live[size:]
        self.pending.append((size, None, self.device.test_started))
        self.device.trace.append({"op": "truncate", "file": self.file.name, "bytes": size})

    def xSync(self, flags):
        mode = self.device.sync_mode if self.file.name.endswith("-wal") else "persist"
        if mode in {"reject", "torn"}:
            event = {"op": "sync_rejected", "file": self.file.name, "flags": flags}
            if mode == "torn":
                writes = [(offset, data) for offset, data, current in self.pending
                          if current and data is not None]
                require(len(writes) >= 2 and len(writes[0][1]) == 24 and len(writes[1][1]) > 24,
                        "expected a WAL frame header followed by a page")
                header, page = writes[:2]
                prefix = page[1][:len(page[1]) // 2]
                super().xWrite(header[1], header[0])
                super().xWrite(prefix, page[0])
                super().xSync(flags)
                event.update(op="sync_torn", page_offset=page[0], bytes=len(prefix),
                             sha256=digest(prefix))
            self.device.trace.append(event)
            raise apsw.exception_for(apsw.SQLITE_IOERR_FSYNC)
        if mode == "lie":
            self.device.trace.append({"op": "sync_dropped", "file": self.file.name, "flags": flags})
            return
        for offset, data, _ in self.pending:
            if data is None:
                super().xTruncate(offset)
            else:
                super().xWrite(data, offset)
        super().xSync(flags)
        self.device.trace.append({"op": "sync", "file": self.file.name, "flags": flags,
                                  "operations": len(self.pending)})
        self.pending.clear()


class PowerVFS(apsw.VFS):
    base = "unix"
    name = "pd-power"

    def __init__(self, root):
        super().__init__(self.name, self.base)
        self.root = root.resolve()
        self.trace = []
        self.sync_mode = "persist"
        self.test_started = False

    def checked_path(self, name):
        path = Path(name.filename() if isinstance(name, apsw.URIFilename) else name)
        require(path.parent.resolve() == self.root and path.name in FILES and not path.is_symlink(),
                f"unexpected SQLite path: {path}")
        return path

    def xOpen(self, name, flags):
        return CachedFile(self, name, flags)

    def xDelete(self, name, syncdir):
        path = self.checked_path(name)
        super().xDelete(name, syncdir)
        self.trace.append({"op": "delete", "file": path.name, "syncdir": bool(syncdir)})

    def cut(self, recovered):
        """Freeze only the backing files, while SQLite still has open handles."""
        images = {}
        for path in sorted(self.root.iterdir()):
            self.checked_path(str(path))
            shutil.copyfile(path, recovered / path.name)
            data = (recovered / path.name).read_bytes()
            images[path.name] = {"bytes": len(data), "sha256": digest(data)}
        self.trace.append({"op": "cut", "images": images})


def rows(db):
    return list(db.execute("SELECT key, value FROM metadata ORDER BY key"))


def run_case(root, mode):
    live, recovered = root / "live", root / "recovered"
    live.mkdir(parents=True)
    recovered.mkdir()
    device = PowerVFS(live)
    db = apsw.Connection(str(device.root / "meta.db"), vfs=device.name)
    try:
        require(db.pragma("locking_mode", "exclusive") == "exclusive", "exclusive lock required")
        require(db.pragma("journal_mode", "wal") == "wal", "WAL mode unavailable")
        db.pragma("synchronous", "FULL")
        db.pragma("wal_autocheckpoint", 0)
        db.execute("CREATE TABLE metadata(key TEXT PRIMARY KEY, value INTEGER)")
        db.execute("INSERT INTO metadata VALUES ('applied', 0), ('revision', 0)")
        require(db.pragma("wal_checkpoint", "TRUNCATE") == (0, 0, 0), "checkpoint failed")
        # Start the new WAL under FULL. NORMAL can sync the WAL header on the
        # first transaction after a reset, so measure an append to a live WAL.
        db.execute("UPDATE metadata SET value = 1")
        db.pragma("synchronous", "NORMAL" if mode == "normal" else "FULL")
        synchronous = db.pragma("synchronous")
        require(synchronous == (1 if mode == "normal" else 2), "synchronous mode mismatch")
        device.sync_mode = mode if mode in {"reject", "lie", "torn"} else "persist"
        start = len(device.trace)
        device.test_started = True
        db.execute("BEGIN")
        db.execute("UPDATE metadata SET value = 2")
        error = None
        ack = root / "client.jsonl"
        with ack.open("x") as log:
            try:
                db.execute("COMMIT")
            except apsw.IOError as exc:
                error = exc.extendedresult
            else:
                require(rows(db) == [("applied", 2), ("revision", 2)], "bad live state")
                log.write(json.dumps({"applied": 2, "revision": 2}) + "\n")
                log.flush()
                os.fsync(log.fileno())
                device.trace.append({"op": "client_ack", "revision": 2})
        device.cut(recovered)
        trace = device.trace[:]
    finally:
        # Any close-time checkpoint only touches the discarded live directory.
        device.sync_mode = "persist"
        db.close(force=True)
        device.unregister()

    with closing(apsw.Connection(str(recovered / "meta.db"), flags=apsw.SQLITE_OPEN_READWRITE)) as reopened:
        require(reopened.pragma("integrity_check") == "ok", "database failed integrity check")
        recovered_rows = dict(rows(reopened))
    confirmed = [json.loads(line) for line in ack.read_text().splitlines()]
    missing = [entry for entry in confirmed if entry != recovered_rows]
    rejected = mode in {"reject", "torn"}
    require(error == (apsw.SQLITE_IOERR_FSYNC if rejected else None), "bad commit result")
    require(len(confirmed) == (0 if rejected else 1), "bad acknowledgement count")
    require(len(missing) == int(mode in {"normal", "lie"}), "bad acknowledgement reconciliation")
    value = 2 if mode == "full" else 1
    require(recovered_rows == {"applied": value, "revision": value}, "bad recovered transaction")
    expected_sync = {"full": "sync", "normal": None, "reject": "sync_rejected",
                     "lie": "sync_dropped", "torn": "sync_torn"}[mode]
    wal_syncs = [event["op"] for event in trace[start:]
                 if event.get("file") == "meta.db-wal" and event["op"].startswith("sync")]
    require(set(wal_syncs) == ({expected_sync} if expected_sync else set()),
            f"{mode}: unexpected WAL sync operations: {wal_syncs}")
    return {"case": mode, "synchronous": synchronous, "commit_error": error,
            "confirmed": confirmed, "missing_confirmed": missing, "recovered": recovered_rows,
            "transaction_start_event": start, "trace": trace}


def main():
    version = apsw.sqlitelibversion()
    source_id = apsw.sqlite3_sourceid()
    require(source_id == SOURCE_IDS.get(version), "SQLite does not match evaluated amalgamation")
    with tempfile.TemporaryDirectory(prefix="pd-sqlite-power-") as work:
        root = Path(work)
        require(root.stat().st_mode & 0o077 == 0, "experiment directory is not private")
        cases = [run_case(root / mode, mode) for mode in ("full", "normal", "reject", "lie", "torn")]
    print(json.dumps({"apsw": apsw.apswversion(), "sqlite": version,
                      "sqlite_source_id": source_id, "cases": cases}, indent=2))


if __name__ == "__main__":
    main()
