# Deliberately break the predicted-load safeguard; restore exact bytes on every exit.
from pathlib import Path
import subprocess
p=Path('pkg/schedule/schedulers/hot_destination.rs')
original=p.read_bytes()
try:
 bad=original.replace(b'let max = detail.load_pred.max();',b'let max = detail.load_pred.min();')
 assert bad!=original
 p.write_bytes(bad)
 result=subprocess.run(['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_090','--','--nocapture'])
finally:
 p.write_bytes(original)
raise SystemExit(result.returncode)
