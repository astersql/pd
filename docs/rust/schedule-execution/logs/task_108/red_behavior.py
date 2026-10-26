# Run only through run.py, inheriting its owned shared Cargo slot.
from pathlib import Path
import subprocess,json,signal
def interrupted(signum, frame):
 raise KeyboardInterrupt
signal.signal(signal.SIGTERM,interrupted)
source=Path('pkg/schedule/schedulers/scheduler_loop.rs')
logs=Path('docs/rust/schedule-execution/logs/task_108')
original=source.read_text()
mutations=[
 ('retry-budget','for attempt in 0..10','for attempt in 0..11'),
 ('backoff','self.interval = self.scheduler.next_interval(self.interval);','self.interval = self.scheduler.min_interval();'),
 ('cleanup','self.policy.clean(cluster);','let _ = cluster;'),
 ('label-exemption','if !self.scheduler.evict_leader()','if true'),
]
results=[]
try:
 for name,before,after in mutations:
  assert before in original,name
  source.write_text(original.replace(before,after,1))
  command=['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_108','--','--nocapture']
  result=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  (logs/f'red-{name}.log').write_text(result.stdout)
  detected=result.returncode==101 and 'panicked at' in result.stdout and 'FAILED' in result.stdout and 'could not compile' not in result.stdout
  results.append(dict(mutation=name,command=command,exit_code=result.returncode,behavior_detected=detected))
  print(name,result.returncode,detected,flush=True)
finally:
 source.write_text(original)
 (logs/'red-results.json').write_text(json.dumps(results,indent=2)+'\n')
assert len(results)==len(mutations) and all(r['behavior_detected'] for r in results)
