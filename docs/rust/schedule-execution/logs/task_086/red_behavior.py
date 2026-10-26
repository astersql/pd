# Deliberate Rust-only mutations demonstrate that the same behavior tests reject regressions.
from pathlib import Path
import subprocess, sys
p=Path('pkg/schedule/schedulers/hot_solution.rs')
original=p.read_bytes()
runner='docs/rust/schedule-execution/logs/task_086/run.py'
try:
 text=original.decode().replace('self.peers_rate(dim) >= 0.', 'self.peers_rate(dim) < 0.').replace('weight * pending_weight','weight * pending_weight * 0.')
 assert text!=original.decode()
 p.write_text(text)
 subprocess.run([sys.executable,runner,'red-fmt','cargo','fmt','--all'],check=True)
 result=subprocess.run([sys.executable,runner,'red-behavior','cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_086','--','--nocapture'])
 assert result.returncode==101
finally:
 p.write_bytes(original)
 subprocess.run([sys.executable,runner,'restored-fmt','cargo','fmt','--all'],check=True)
