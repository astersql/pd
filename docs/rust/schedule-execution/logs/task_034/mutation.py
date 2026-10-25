"""Prove the task's same-priority assertion detects an incorrect comparator."""
from pathlib import Path
import subprocess,json
out=Path('docs/rust/schedule-execution/logs/task_034')
p=Path('pkg/schedule/operator/controller_admission.rs'); original=p.read_text()
try:
    needle='op.priority_level().as_i32() <= old.priority_level().as_i32()'
    assert needle in original
    p.write_text(original.replace(needle,needle.replace('<=','<')))
    subprocess.run(['cargo','fmt','--all'],check=True)
    argv=['cargo','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_034','--','--nocapture']
    result=subprocess.run(argv,capture_output=True,text=True)
    (out/'red.out').write_text(result.stdout+result.stderr)
    (out/'red.json').write_text(json.dumps(dict(argv=argv,exit_code=result.returncode,mutation='allow equal priority to replace existing operator'),indent=2)+'\n')
    assert result.returncode==101
    assert 'must reject' in result.stdout + result.stderr
finally:
    p.write_text(original)
    subprocess.run(['cargo','fmt','--all'],check=True)
