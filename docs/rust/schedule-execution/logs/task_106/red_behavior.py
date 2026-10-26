# Invoked inside a leased Cargo slot by run.py. Restore source even on failure.
from pathlib import Path
import subprocess,json
path=Path('pkg/schedule/schedulers/registry.rs')
original=path.read_bytes()
logs=Path('docs/rust/schedule-execution/logs/task_106')
results=[]
try:
 for name,old,new in [
  ('wrong-batch','get_key_ranges(args).map_err(invalid)?, 4','get_key_ranges(args).map_err(invalid)?, 5'),
  ('wrong-type','BuiltinScheduler::ShuffleLeader(_) => SHUFFLE_LEADER_SCHEDULER','BuiltinScheduler::ShuffleLeader(_) => LABEL_SCHEDULER')]:
  source=original.decode()
  assert old in source,old
  path.write_text(source.replace(old,new,1))
  subprocess.run(['cargo','fmt','--all'],check=True)
  cmd=['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_106','--','--nocapture']
  with (logs/f'red-{name}.log').open('w') as output:
   result=subprocess.run(cmd,stdout=output,stderr=subprocess.STDOUT)
  log=(logs/f'red-{name}.log').read_text()
  assert result.returncode==101 and 'test result: FAILED' in log and 'assertion `left == right` failed' in log,log[-4000:]
  results.append(dict(mutation=name,command=cmd,exit_code=result.returncode,assertion_failure=True))
  path.write_bytes(original)
finally:
 path.write_bytes(original)
 (logs/'red-results.json').write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps(results))
