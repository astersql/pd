# Deliberately wrong median; restore source even if interrupted.
from pathlib import Path
import subprocess, sys
p=Path('pkg/schedule/schedulers/hot_split.rs')
original=p.read_bytes()
try:
 text=original.decode()
 assert 'keys[keys.len() / 2]' in text
 p.write_text(text.replace('keys[keys.len() / 2]', 'keys[0]'))
 result=subprocess.run(['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_093','--','--nocapture'])
 assert result.returncode == 101, result.returncode
finally:
 p.write_bytes(original)
