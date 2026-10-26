from pathlib import Path
import subprocess
p=Path('pkg/schedule/schedulers/label.rs');original=p.read_bytes()
try:
 s=original.decode();old='ctx.config.check_label_property(REJECT_LEADER, s.labels())';assert old in s
 p.write_text(s.replace(old,'!'+old))
 assert subprocess.run(['cargo','fmt','--all']).returncode==0
 code=subprocess.run(['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_097_behavior_cases','--','--nocapture']).returncode
 assert code==101,code
 print('EXPECTED behavioral assertion failure, cargo exit 101')
finally:
 p.write_bytes(original)
