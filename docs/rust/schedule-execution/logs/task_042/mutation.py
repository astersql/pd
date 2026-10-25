"""Prove the same behavior test rejects wrong count, switch, priority and retry logic."""
from pathlib import Path
import subprocess,json,os
root=Path.cwd();out=Path(__file__).resolve().parent
p=root/'pkg/schedule/checker/replica_shape.rs';original=p.read_text()
assert os.environ['CARGO_TARGET_DIR'].startswith(str(root/'target/rust-slot-'))
changes=[
 ('count-learners-as-voters','region.voters().len()','region.meta().peers.len()'),
 ('ignore-add-disable','context.config.is_make_up_replica_enabled()','true'),
 ('no-make-up-priority','op.set_priority_level(PriorityLevel::HIGH);','op.set_priority_level(PriorityLevel::MEDIUM);'),
 ('drop-pending','out.pending |= temp;','out.pending |= false;'),
 ('disable-location','if context.config.is_location_replacement_enabled()','if false'),
]
records=[]
try:
 for name,before,after in changes:
  assert before in original
  p.write_text(original.replace(before,after,1))
  subprocess.run(['cargo','fmt','--all'],check=True)
  cmd=['cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_042','--','--nocapture']
  r=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  (out/f'mutation-{name}.log').write_text(r.stdout)
  assert r.returncode==101 and 'task_042_test.rs:' in r.stdout and 'FAILED' in r.stdout, r.stdout
  records.append(dict(mutation=name,command=cmd,exit_code=r.returncode,CARGO_TARGET_DIR=os.environ['CARGO_TARGET_DIR']))
  (out/'mutations.json').write_text(json.dumps(records,indent=2)+'\n')
finally:
 p.write_text(original)
 subprocess.run(['cargo','fmt','--all'],check=True)
print('5 wrong-behavior mutations rejected by the same test')
