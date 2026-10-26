from pathlib import Path
import subprocess
p=Path('pkg/schedule/schedulers/hot_rank_v2.rs')
before=p.read_bytes()
try:
 text=before.decode()
 assert '(1.., 1..) => 4' in text
 p.write_text(text.replace('(1.., 1..) => 4','(1.., 1..) => -1'))
 result=subprocess.run(['python3','docs/rust/schedule-execution/logs/task_088/run.py','red-behavior','cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_088','--','--nocapture'])
 assert result.returncode==101
finally:
 p.write_bytes(before)
