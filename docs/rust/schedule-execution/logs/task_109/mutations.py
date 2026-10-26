from pathlib import Path
import subprocess,json,sys
p=Path('pkg/schedule/schedulers/diagnostic_record.rs')
original=p.read_text()
logs=Path('docs/rust/schedule-execution/logs/task_109')
mutations=[
 ('red-capacity','const MAX_RESULTS: usize = 10;','const MAX_RESULTS: usize = 11;','task_109_history'),
 ('red-history','.take_while(|r| r.status == first.status)','.take_while(|_| true)','task_109_history'),
 ('red-weight','if i < first_end {\n                3','if i < first_end {\n                1','task_109_history'),
 ('red-operators','if !operators.is_empty() {','if operators.is_empty() {','task_109_real_dry_run'),
 ('red-normal','if normal {','if !normal {','task_109_behavior_cases'),
]
results=[]
try:
 for name,old,new,test in mutations:
  assert old in original,name
  p.write_text(original.replace(old,new,1))
  cmd=['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib',test,'--','--nocapture']
  with (logs/(name+'.log')).open('w') as out:
   rc=subprocess.call(cmd,stdout=out,stderr=subprocess.STDOUT)
  output=(logs/(name+'.log')).read_text()
  ok=rc==101 and 'panicked at' in output and 'FAILED' in output and 'test result: FAILED' in output
  results.append(dict(name=name,command=cmd,exit_code=rc,assertion_failure=ok))
  if not ok: raise RuntimeError(name+' failed to detect mutation')
finally:
 p.write_text(original)
 (logs/'mutations.json').write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps(results))
