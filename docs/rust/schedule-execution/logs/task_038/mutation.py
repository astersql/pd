"""Verify the same tests detect a wrong peer role, then restore source."""
from pathlib import Path
import subprocess,sys
root=Path.cwd();source=root/'pkg/schedule/operator/apply_steps.rs'
original=source.read_text()
try:
    wrong=original.replace('role: PeerRole::Learner,','role: PeerRole::Voter,')
    assert wrong != original
    source.write_text(wrong)
    result=subprocess.run([sys.executable,'docs/rust/schedule-execution/logs/task_038/run.py','mutation','cargo','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_038','--','--nocapture'])
    assert result.returncode == 101
finally:
    source.write_text(original)
