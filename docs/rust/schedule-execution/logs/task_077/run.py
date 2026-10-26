# Shared Cargo slot orchestration; run only from the repository root.
from pathlib import Path
import os,sys,subprocess,json,time,signal
def interrupted(signum, frame):
 raise KeyboardInterrupt
signal.signal(signal.SIGTERM, interrupted)
root=Path.cwd().resolve()
logs=root/'docs/rust/schedule-execution/logs/task_077'
locks=root/'target/rust-slot-locks'
locks.mkdir(parents=True,exist_ok=True)
while True:
 for slot in range(1,11):
  lock=locks/f'slot-{slot}.lock'
  try: lock.mkdir()
  except FileExistsError: continue
  break
 else:
  time.sleep(1)
  continue
 break
owner=lock/'owner.json'
owner.write_text(json.dumps(dict(plan='调度执行内核',task=77,session='task-077',pid=os.getpid())))
env=os.environ.copy()
env['CARGO_TARGET_DIR']=str(root/f'target/rust-slot-{slot}')
name=sys.argv[1]
cmd=sys.argv[2:]
try:
 with (logs/f'{name}.log').open('w') as output:
  child=subprocess.Popen(cmd,env=env,stdout=output,stderr=subprocess.STDOUT,start_new_session=True)
  prior=set(Path(env['CARGO_TARGET_DIR']).glob('rust-etcd-fixture-*'))
  try:
   while child.poll() is None:
    time.sleep(0.25)
    for binary in set(Path(env['CARGO_TARGET_DIR']).glob('rust-etcd-fixture-*'))-prior:
     pid=int(binary.name.rsplit('-',1)[1])
     try: os.kill(pid,0)
     except ProcessLookupError: binary.unlink(missing_ok=True)
  except BaseException:
   os.killpg(child.pid,signal.SIGTERM)
   try: child.wait(timeout=5)
   except subprocess.TimeoutExpired:
    os.killpg(child.pid,signal.SIGKILL)
    child.wait()
   raise
  result=child
 record=dict(command=cmd,exit_code=result.returncode,slot=slot,CARGO_TARGET_DIR=env['CARGO_TARGET_DIR'],CARGO_BUILD_JOBS=env.get('CARGO_BUILD_JOBS'),RUST_TEST_THREADS=env.get('RUST_TEST_THREADS'))
 (logs/f'{name}.json').write_text(json.dumps(record,indent=2)+'\n')
 print(json.dumps(record))
 print((logs/f'{name}.log').read_text()[-3500:])
finally:
 owner.unlink()
 lock.rmdir()

sys.exit(result.returncode)
