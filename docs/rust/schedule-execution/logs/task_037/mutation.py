"""Verify the same behavior tests reject an incorrect terminal metric event."""
from pathlib import Path
import subprocess, sys
root = Path(__file__).resolve().parents[5]
source = root / 'pkg/schedule/operator/controller_records.rs'
original = source.read_bytes()
try:
    source.write_bytes(original.replace(b'SUCCESS => "finish"', b'SUCCESS => "cancel"'))
    code = subprocess.call([sys.executable, str(Path(__file__).with_name('run.py')), 'mutation', 'sh', '-c', 'cargo fmt --all && cargo test -p astersql-pd-schedule-operator --locked --lib task_037 -- --nocapture'], cwd=root)
    if code != 101:
        raise RuntimeError(f'expected assertion failure exit101, got {code}')
finally:
    source.write_bytes(original)
