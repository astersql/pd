"""Verify that frozen behavior cases reject a removal ID/finish conflation."""
from pathlib import Path
import subprocess,json,os
p=Path('pkg/schedule/operator/step_remove.rs');original=p.read_text()
out=Path('docs/rust/schedule-execution/logs/task_020')
try:
    needle='region.get_store_peer(self.from_store).is_none()'
    assert original.count(needle)==1
    p.write_text(original.replace(needle,'region.get_store_peer(self.from_store).is_none_or(|p| p.id != self.peer_id)'))
    subprocess.run(['cargo','fmt','--all'],check=True)
    argv=['cargo','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_020','--','--nocapture']
    result=subprocess.run(argv,capture_output=True,text=True)
    log=result.stdout+result.stderr
    (out/'red-behavior.out').write_text(log)
    assert result.returncode==101 and 'wrong-voter' in log and 'task_020_behavior_cases ... FAILED' in log
    (out/'red.json').write_text(json.dumps(dict(argv=argv,exit_code=result.returncode,mutation='treat a replacement peer ID as finished removal',expected='wrong-voter must still be unfinished even when confver changed',cargo_target_dir=os.environ['CARGO_TARGET_DIR']),indent=2)+'\n')
finally:
    p.write_text(original)
    subprocess.run(['cargo','fmt','--all'],check=True)
