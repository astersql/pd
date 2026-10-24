#!/bin/bash
# Atomic shared-slot acquisition; terminate/wait the command group before release.
exec python3 - "$@" <<'PY'
import os, pathlib, signal, subprocess, sys, time
root=pathlib.Path.cwd()
assert (root/'server/config/Cargo.toml').exists(), 'run from repository root'
locks=root/'target/rust-slot-locks'
locks.mkdir(parents=True,exist_ok=True)
lock=None
child=None
def interrupted(signum, frame):
    raise SystemExit(128+signum)
signal.signal(signal.SIGINT,interrupted)
signal.signal(signal.SIGTERM,interrupted)
try:
    while lock is None:
        for n in range(1,11):
            candidate=locks/f'slot-{n}.lock'
            if subprocess.run(['mkdir',str(candidate)],stderr=subprocess.DEVNULL).returncode==0:
                lock=candidate
                break
        if lock is None: time.sleep(1)
    (lock/'owner').write_text(f'plan=统计状态 task=15 session=state15 PID={os.getpid()}\n')
    os.environ['CARGO_TARGET_DIR']=str(root/f'target/rust-slot-{n}')
    os.environ.update(GOARCH='arm64',CGO_ENABLED='0',GOFLAGS='-mod=readonly',GOPROXY='off')
    print(f'SLOT={n} CARGO_TARGET_DIR={os.environ["CARGO_TARGET_DIR"]}',flush=True)
    child=subprocess.Popen(sys.argv[1:],start_new_session=True)
    sys.exit(child.wait())
finally:
    if child is not None and child.poll() is None:
        os.killpg(child.pid,signal.SIGTERM)
        try: child.wait(timeout=10)
        except subprocess.TimeoutExpired:
            os.killpg(child.pid,signal.SIGKILL)
            child.wait()
    if lock is not None:
        (lock/'owner').unlink(missing_ok=True)
        lock.rmdir()
PY
