# Behavioral mutation must fail the same fixture test; always restore exact bytes.
from pathlib import Path
import subprocess, signal
def interrupted(signum, frame):
 raise KeyboardInterrupt
signal.signal(signal.SIGTERM, interrupted)
p=Path('pkg/schedule/schedulers/transfer_witness.rs')
original=p.read_bytes()
try:
 p.write_bytes(original.replace(b'op.set_priority_level(PriorityLevel::URGENT)',b'op.set_priority_level(PriorityLevel::LOW)'))
 result=subprocess.run(['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_103_behavior_cases','--','--nocapture'],capture_output=True,text=True)
 print(result.stdout,result.stderr)
 print('mutation cargo exit:',result.returncode)
 assert result.returncode==101 and 'assertion `left == right` failed' in result.stdout+result.stderr
finally:
 p.write_bytes(original)
