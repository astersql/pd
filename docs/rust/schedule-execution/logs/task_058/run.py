# Shared Cargo slot orchestration; run only from the repository root.
from pathlib import Path
import os,sys,subprocess,json,time,signal
def interrupted(signum, frame):
 raise KeyboardInterrupt
signal.signal(signal.SIGTERM, interrupted)
root=Path.cwd().resolve()
logs=root/'docs/rust/schedule-execution/logs/task_058'
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
owner.write_text(json.dumps(dict(plan='调度执行内核',task=58,session='task-058',pid=os.getpid())))
env=os.environ.copy()
env['CARGO_TARGET_DIR']=str(root/f'target/rust-slot-{slot}')
name=sys.argv[1]
cmd=sys.argv[2:]
try:
 with (logs/f'{name}.log').open('w') as output:
  result=subprocess.run(cmd,env=env,stdout=output,stderr=subprocess.STDOUT)
 record=dict(command=cmd,exit_code=result.returncode,slot=slot,CARGO_TARGET_DIR=env['CARGO_TARGET_DIR'])
 (logs/f'{name}.json').write_text(json.dumps(record,indent=2)+'\n')
 print(json.dumps(record))
 print((logs/f'{name}.log').read_text()[-3500:])
finally:
 owner.unlink()
 lock.rmdir()
