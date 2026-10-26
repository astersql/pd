# Deliberately broaden the population; restore the production source on every exit.
from pathlib import Path
import subprocess
p=Path('pkg/schedule/schedulers/hot_placement.rs')
original=p.read_bytes()
try:
 s=original.decode()
 old='if !rules\n                .iter()\n                .any(|r| match_label_constraints(Some(store), &r.label_constraints))'
 assert old in s
 p.write_text(s.replace(old,'if false',1))
 subprocess.run(['cargo','fmt','--all'],check=True)
 result=subprocess.run(['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_091','--','--nocapture'])
 assert result.returncode!=0
 print('Mutation detected: broadened population rejected by unchanged behavior assertions',flush=True)
finally:
 p.write_bytes(original)
