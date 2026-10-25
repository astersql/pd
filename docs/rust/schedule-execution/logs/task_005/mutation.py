"""Verify these same behavior tests reject incorrect selection and restore source."""
import json, os, re, subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_005')
p=Path('pkg/schedule/filter/selection.rs');original=p.read_text();results=[]
cmd=['cargo','test','-p','astersql-pd-schedule-filter','--locked','--lib','task_005','--','--nocapture']
mutations=[
 ('source-target', 'Self::status(self.sources.contains(&store.id()))', 'Self::status(self.targets.contains(&store.id()))'),
 ('last-rejection', '.iter().enumerate().find_map', '.iter().enumerate().rev().find_map'),
 ('target-step', 'PlanOption::SetResourceWithStep(resource, 2)', 'PlanOption::SetResource(resource)'),
 ('selection-inversion', 'if rejected == unavailable', 'if rejected != unavailable'),
]
try:
 for name,old,new in mutations:
  assert original.count(old)==1,(name,old)
  p.write_text(original.replace(old,new))
  subprocess.run(['cargo','fmt','--all'],check=True,capture_output=True)
  r=subprocess.run(cmd,capture_output=True,text=True);raw=r.stdout+r.stderr
  (out/(name+'.out')).write_text(raw)
  counts=re.search(r'test result: FAILED\. (\d+) passed; (\d+) failed',raw)
  record=dict(name=name,argv=cmd,exit_code=r.returncode,cargo_target_dir=os.environ['CARGO_TARGET_DIR'],passed=int(counts[1]) if counts else 0,failed=int(counts[2]) if counts else 0)
  results.append(record);(out/'mutation.json').write_text(json.dumps(results,indent=2)+'\n');print(record,flush=True)
  assert r.returncode==101 and counts and int(counts[2])>0 and 'error[E' not in raw
finally:
 p.write_text(original)
 subprocess.run(['cargo','fmt','--all'],check=True)
