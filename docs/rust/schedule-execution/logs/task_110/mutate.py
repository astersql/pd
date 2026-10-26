# Executed by run.py inside one owned shared Cargo slot.
from pathlib import Path
import subprocess,json,os
path=Path('pkg/schedule/schedulers/scheduler_metrics.rs')
original=path.read_text()
logs=Path('docs/rust/schedule-execution/logs/task_110')
mutations=[('eager','for &(typ, event) in INITIAL_EVENTS {','for &(typ, event) in INITIAL_EVENTS.iter().take(0) {'),('reset','if family.definition.reset {','if false {'),('delete','labels[index] == store','labels[index] != store'),('event','self.schedule.inc();','// deliberately omit event'),('bury','if cluster.get_store(id).is_none_or(|s| s.is_removed()) {','if false {')]
records=[]
try:
 for name,before,after in mutations:
  assert before in original
  if name=='bury':
   index=original.rfind(before);mutated=original[:index]+original[index:].replace(before,after,1)
  else: mutated=original.replace(before,after,1)
  path.write_text(mutated)
  assert subprocess.run(['cargo','fmt','--all'],stdout=subprocess.DEVNULL).returncode==0
  cmd=['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_110','--','--nocapture']
  out=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  (logs/f'red-{name}.log').write_text(out.stdout)
  actual=out.returncode==101 and 'test result: FAILED' in out.stdout and 'panicked at' in out.stdout and 'could not compile' not in out.stdout
  records.append(dict(name=name,command=cmd,exit_code=out.returncode,behavior_assertion_failed=actual,slot=os.environ['CARGO_TARGET_DIR']))
  assert actual, out.stdout[-3000:]
finally:
 path.write_text(original)
 (logs/'behavior-mutations.json').write_text(json.dumps(records,indent=2)+'\n')
print(json.dumps(records))
