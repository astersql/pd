"""Prove the fixed completion test rejects lost pending-voter protection."""
from pathlib import Path
import subprocess,json,os
p=Path('pkg/schedule/operator/step_add.rs');original=p.read_text()
out=Path('docs/rust/schedule-execution/logs/task_019')
try:
    needle='&& r.get_pending_voter(p.id).is_none()'
    assert original.count(needle)==1
    p.write_text(original.replace(needle,''))
    subprocess.run(['cargo','fmt','--all'],check=True)
    argv=['cargo','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_019','--','--nocapture']
    result=subprocess.run(argv,capture_output=True,text=True)
    log=result.stdout+result.stderr
    (out/'red-behavior.out').write_text(log)
    assert result.returncode==101 and 'pending-voter' in log and 'task_019_behavior_cases ... FAILED' in log
    (out/'red.json').write_text(json.dumps(dict(argv=argv,exit_code=result.returncode,mutation='omit AddPeer pending-voter completion guard',expected='pending-voter finished=true differs from frozen false',cargo_target_dir=os.environ['CARGO_TARGET_DIR']),indent=2)+'\n')
finally:
    p.write_text(original)
    subprocess.run(['cargo','fmt','--all'],check=True)
