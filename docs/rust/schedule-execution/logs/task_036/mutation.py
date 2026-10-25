"""Prove the same behavior tests reject disabled epoch validation; restore on exit."""
from pathlib import Path
import subprocess
root=Path(__file__).resolve().parents[5]
runner=root/'docs/rust/schedule-execution/logs/task_036/run.py'
p=root/'pkg/schedule/operator/controller_dispatch.rs'
s=p.read_text()
needle='if latest.conf_ver.wrapping_sub(origin.conf_ver) > op.conf_ver_changed(r) {'
assert needle in s
run=lambda name,*args: subprocess.run(['python3',str(runner),name,'cargo',*args],cwd=root).returncode
try:
    p.write_text(s.replace(needle,'if false && latest.conf_ver.wrapping_sub(origin.conf_ver) > op.conf_ver_changed(r) {'))
    assert run('mutation-fmt','fmt','--all')==0
    assert run('mutation','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_036','--','--nocapture')==101
finally:
    p.write_text(s)
    assert run('fmt','fmt','--all')==0
assert run('focused','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_036','--','--nocapture')==0
