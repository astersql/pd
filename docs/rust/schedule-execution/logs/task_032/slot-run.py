"""Atomically claim a repository Cargo slot and join the command before release."""
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import sys
import time

root = Path(subprocess.check_output(['git', 'rev-parse', '--show-toplevel'], text=True).strip())
os.chdir(root)
locks = root / 'target/rust-slot-locks'
locks.mkdir(parents=True, exist_ok=True)
lock = None
child = None

def stop(signum, _frame):
    if child is not None and child.poll() is None:
        os.killpg(child.pid, signum)
        try:
            child.wait(timeout=30)
        except subprocess.TimeoutExpired:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait()
    raise SystemExit(128 + signum)

signal.signal(signal.SIGINT, stop)
signal.signal(signal.SIGTERM, stop)
try:
    while lock is None:
        for slot in range(1, 11):
            candidate = locks / f'slot-{slot}.lock'
            try:
                candidate.mkdir()
            except FileExistsError:
                continue
            lock = candidate
            target = root / f'target/rust-slot-{slot}'
            os.environ['CARGO_TARGET_DIR'] = str(target)
            (lock / 'owner').write_text(json.dumps(dict(
                plan='schedule-execution', task='032', session='schedule-range-factory-032',
                pid=os.getpid(), cargo_target_dir=str(target),
            )) + '\n')
            print(f'slot={slot} CARGO_TARGET_DIR={target} command={shlex.join(sys.argv[1:])}', flush=True)
            break
        if lock is None:
            time.sleep(2)
    child = subprocess.Popen(sys.argv[1:], start_new_session=True)
    code = child.wait()
    print(f'command_exit_code={code}', flush=True)
    raise SystemExit(code)
finally:
    if child is not None and child.poll() is None:
        child.wait()
    if lock is not None:
        (lock / 'owner').unlink(missing_ok=True)
        lock.rmdir()
