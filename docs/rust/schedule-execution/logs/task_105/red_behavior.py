from pathlib import Path
import subprocess,json,signal
def interrupted(signum,frame):
 raise KeyboardInterrupt
signal.signal(signal.SIGTERM,interrupted)
source=Path('pkg/schedule/schedulers/balance_range_schedule.rs')
original=source.read_bytes()
text=original.decode()
mutations=[('improvement','source - source_influence.unsigned_abs() as f64 - 1.0','source - source_influence.unsigned_abs() as f64 + 1.0','task_105_behavior_cases'),('report','now.0 - last.0 < 10_000_000_000','now.0 - last.0 <= 10_000_000_000','task_105_roles_engines_and_faults')]
records=[]
try:
 for name,old,new,test in mutations:
  assert old in text,old
  source.write_text(text.replace(old,new,1))
  cmd=['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib',test,'--','--nocapture']
  r=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  Path(f'docs/rust/schedule-execution/logs/task_105/red-{name}.log').write_text(r.stdout)
  assert r.returncode==101 and 'test result: FAILED.' in r.stdout,r.stdout
  records.append(dict(mutation=name,command=cmd,exit_code=r.returncode))
finally:
 source.write_bytes(original)
Path('docs/rust/schedule-execution/logs/task_105/red-results.json').write_text(json.dumps(records,indent=2)+'\n')
print(json.dumps(records))
