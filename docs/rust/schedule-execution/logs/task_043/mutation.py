"""Verify the same behavioral suite rejects wrong production behavior; restore on exit."""
from pathlib import Path
import subprocess,json,os
out=Path(__file__).resolve().parent;root=out.parents[4];src=root/'pkg/schedule/checker/rule_missing.rs';original=src.read_text()
changes=[('skip-missing','if (rf.peers.len() as i64) < rf.rule.count {','if false && (rf.peers.len() as i64) < rf.rule.count {'),('learner-as-voter','role: if rule.role == PeerRoleType::Learner {','role: if false && rule.role == PeerRoleType::Learner {'),('lose-priority','op.set_priority_level(PriorityLevel::HIGH);','op.set_priority_level(PriorityLevel::MEDIUM);'),('lose-pending','out.pending = !temp;','out.pending = false;'),('tiflash-accept-compute','c.values == ["tiflash"]','!c.values.is_empty()')]
records=[]
try:
 for name,old,new in changes:
  assert old in original,name
  src.write_text(original.replace(old,new))
  with (out/f'mutation-{name}.log').open('w') as log:
   p=subprocess.run(['cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_043','--','--nocapture'],cwd=root,stdout=log,stderr=subprocess.STDOUT)
  text=(out/f'mutation-{name}.log').read_text()
  record=dict(name=name,exit_code=p.returncode,behavior_assertion_failed='panicked at' in text and 'FAILED' in text)
  records.append(record);print(json.dumps(record),flush=True)
  assert p.returncode==101 and record['behavior_assertion_failed'],text
finally:
 src.write_text(original)
 (out/'mutations.json').write_text(json.dumps(records,indent=2)+'\n')
