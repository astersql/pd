"""Prove assertions detect wrong task45 behavior; caller owns a shared Cargo slot."""
from pathlib import Path
import subprocess,os,json
out=Path(__file__).resolve().parent
root=out.parents[4]
assert os.environ['CARGO_TARGET_DIR'].startswith(str(root/'target/rust-slot-'))
source=root/'pkg/schedule/checker/rule_location.rs'
original=source.read_text()
mutations=[
 ('ignore-fit-health','if !unhealthy_fit {','if true {'),
 ('ignore-replacement-fit','if !fit.replace(', 'if false && !fit.replace('),
 ('ignore-extra-count','fit.extra_count() > 0','true'),
 ('ignore-disconnected-priority','disconnected_peer.unwrap_or(orphan)','orphan'),
 ('invert-filter-state','out.pending = !temporary;','out.pending = temporary;'),
 ('omit-witness-flag','let witness = rf.rule.is_witness && ctx.witness_enabled;','let witness = false;'),
]
records=[]
try:
 for name,old,new in mutations:
  assert original.count(old)==1,(name,original.count(old))
  source.write_text(original.replace(old,new))
  cmd=['cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_045','--','--nocapture']
  with (out/f'mutation-{name}.log').open('w') as log:
   p=subprocess.Popen(cmd,cwd=root,stdout=log,stderr=subprocess.STDOUT)
   try:code=p.wait()
   except BaseException:
    p.terminate();p.wait();raise
  text=(out/f'mutation-{name}.log').read_text()
  record=dict(name=name,command=cmd,exit_code=code,behavior_failure='panicked at pkg/schedule/checker/task_045_test.rs:' in text and 'error: could not compile' not in text,CARGO_TARGET_DIR=os.environ['CARGO_TARGET_DIR'])
  records.append(record);(out/'mutations.json').write_text(json.dumps(records,indent=2)+'\n')
  assert code==101 and record['behavior_failure'],record
  print(json.dumps(record),flush=True)
finally:
 source.write_text(original)
