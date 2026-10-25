import json,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_016')
p=Path('pkg/schedule/operator/status.rs');original=p.read_text();results=[]
mutations=[('equal-deadline', 'now.saturating_duration_since(reached) < duration','now.saturating_duration_since(reached) <= duration'),('creation-instead-of-start', '.time_of(from)', '.time_of(CREATED)')]
try:
 for name,old,new in mutations:
  assert old in original
  p.write_text(original.replace(old,new))
  subprocess.run(['cargo','fmt','--all'],check=True)
  args=['cargo','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_016','--','--nocapture']
  r=subprocess.run(args,capture_output=True,text=True)
  log=r.stdout+r.stderr;(out/f'red-{name}.out').write_text(log)
  assert r.returncode==101 and 'assertion `left == right` failed' in log or r.returncode==101 and 'assertion failed:' in log
  results.append(dict(mutation=name,argv=args,exit_code=r.returncode,log=f'red-{name}.out'))
  print(name,r.returncode,flush=True)
finally:
 p.write_text(original)
 subprocess.run(['cargo','fmt','--all'],check=True)
 (out/'red.json').write_text(json.dumps(results,indent=2)+'\n')
