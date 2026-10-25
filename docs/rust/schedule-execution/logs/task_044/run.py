"""Own one shared Cargo slot for a single command, including failure cleanup."""
from pathlib import Path
import os,sys,subprocess,time,json,signal

def interrupted(signum, frame):
    raise KeyboardInterrupt(f"signal {signum}")
signal.signal(signal.SIGTERM, interrupted)
root=Path(__file__).resolve().parents[5]
locks=root/'target/rust-slot-locks';locks.mkdir(parents=True,exist_ok=True)
while True:
    for n in range(1,11):
        lock=locks/f'slot-{n}.lock'
        try: lock.mkdir()
        except FileExistsError: continue
        break
    else:
        time.sleep(2);continue
    break
out=Path(__file__).resolve().parent
try:
    (lock/'owner.json').write_text(json.dumps(dict(plan='schedule-execution',task=44,session='task-044',pid=os.getpid())))
    env=dict(os.environ,CARGO_TARGET_DIR=str(root/f'target/rust-slot-{n}'))
    name=sys.argv[1];cmd=sys.argv[2:]
    (out/f"{name}.json").unlink(missing_ok=True)
    with (out/f'{name}.log').open('w') as log:
        process=subprocess.Popen(cmd,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
        try: code=process.wait()
        except BaseException:
            try: os.killpg(process.pid,signal.SIGTERM)
            except ProcessLookupError: pass
            try: process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid,signal.SIGKILL);process.wait()
            raise
    record=dict(command=cmd,exit_code=code,slot=n,CARGO_TARGET_DIR=env['CARGO_TARGET_DIR'],test_env={k:env[k] for k in ['RUST_TEST_THREADS'] if k in env})
    (out/f'{name}.json').write_text(json.dumps(record,indent=2)+'\n')
    print(json.dumps(record));print((out/f'{name}.log').read_text()[-3500:])
finally:
    (lock/'owner.json').unlink(missing_ok=True);lock.rmdir()
sys.exit(code)
