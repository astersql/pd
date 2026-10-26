from pathlib import Path
import subprocess
p=Path('pkg/schedule/schedulers/shuffle_leader.rs')
original=p.read_bytes()
try:
    text=original.decode()
    old='op.set_priority_level(PriorityLevel::LOW);'
    assert old in text
    p.write_text(text.replace(old,'op.set_priority_level(PriorityLevel::HIGH);'))
    assert subprocess.run(['cargo','fmt','--all']).returncode==0
    code=subprocess.run(['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_101_behavior_cases','--','--nocapture']).returncode
    assert code==101, code
    print('EXPECTED behavioral priority assertion failure; cargo exit 101')
finally:
    p.write_bytes(original)
