# Prove the frozen behavior assertions reject a wrong production ranking branch.
from pathlib import Path
import subprocess
p=Path('pkg/schedule/schedulers/hot_rank_v1.rs')
original=p.read_bytes()
try:
 text=original.decode()
 assert '(true, _, true, _) => 4' in text
 p.write_text(text.replace('(true, _, true, _) => 4','(true, _, true, _) => -1'))
 result=subprocess.run(['python3','docs/rust/schedule-execution/logs/task_087/run.py','red-behavior','cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_087','--','--nocapture'])
 assert result.returncode==101
finally:
 p.write_bytes(original)
