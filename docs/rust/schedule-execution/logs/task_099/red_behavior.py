from pathlib import Path
import subprocess
p=Path('pkg/schedule/schedulers/scatter_range.rs');original=p.read_bytes()
try:
 s=original.decode();old='local.schedule.tolerant_size_ratio = 2.0;';assert old in s
 p.write_text(s.replace(old,'local.schedule.tolerant_size_ratio = 10000.0;'))
 assert subprocess.run(['cargo','fmt','--all']).returncode==0
 code=subprocess.run(['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_099_behavior_cases','--','--nocapture']).returncode
 assert code==101,code
 print('EXPECTED behavioral assertion failure; cargo exit 101')
finally:
 p.write_bytes(original)
