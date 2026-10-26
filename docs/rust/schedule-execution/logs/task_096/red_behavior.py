from pathlib import Path
import subprocess
p=Path('pkg/schedule/schedulers/grant_hot.rs');original=p.read_bytes()
try:
 s=original.decode();assert 'self.config.store_ids.contains(&src)' in s
 p.write_text(s.replace('self.config.store_ids.contains(&src)','!self.config.store_ids.contains(&src)'))
 assert subprocess.run(['cargo','fmt','--all']).returncode==0
 code=subprocess.run(['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_096_behavior_cases','--','--nocapture']).returncode
 assert code==101,code
 print('EXPECTED behavioral assertion failure, cargo exit 101')
finally:
 p.write_bytes(original)
