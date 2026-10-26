from pathlib import Path
import subprocess
p=Path('pkg/schedule/schedulers/shuffle_region.rs')
original=p.read_bytes()
try:
 text=original.decode()
 assert 'op.set_priority_level(PriorityLevel::LOW)' in text
 p.write_text(text.replace('op.set_priority_level(PriorityLevel::LOW)','op.set_priority_level(PriorityLevel::HIGH)'))
 subprocess.run(['cargo','fmt','--all'],check=True)
 result=subprocess.run(['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_102_behavior_cases','--','--nocapture'])
 assert result.returncode == 101, result.returncode
 print('Expected behavior assertion failure: Cargo exit101; mutation rejected.')
finally:
 p.write_bytes(original)
