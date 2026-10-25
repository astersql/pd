"""Prove lifecycle assertions detect regressions; restore source even on failure."""
from pathlib import Path
import subprocess, json
root=Path(__file__).resolve().parents[5]
out=Path(__file__).resolve().parent
source=root/'pkg/schedule/checker/patrol.rs'
original=source.read_text()
cases=[('priority','for id in host.priority_ids().await {','for id in host.priority_ids().await.into_iter().take(0) {','task_056_real_priority_pending_and_scan_order'),('cursor','state.cursor = region.meta().key_range.end_key.clone();','state.cursor.clear();','task_056_cursor_backlog_dynamic_tick_and_workers'),('join','let joined = drain(&mut workers).await.map_err(PatrolError::Host);','workers.clear(); let joined = Ok(());','task_056_shutdown_and_resize_wait_for_inflight')]
results=[]
try:
    for name,old,new,test in cases:
        assert original.count(old)==1,(name,original.count(old))
        source.write_text(original.replace(old,new))
        cmd=['python3',str(out/'run.py'),'mutation-'+name,'cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib',test,'--','--nocapture']
        p=subprocess.run(cmd,cwd=root)
        log=(out/f'mutation-{name}.log').read_text()
        results.append(dict(name=name,exit_code=p.returncode,assertion_failure='panicked at' in log and 'FAILED' in log))
        assert p.returncode==101 and results[-1]['assertion_failure'],name
finally:
    source.write_text(original)
    (out/'mutation-results.json').write_text(json.dumps(results,indent=2)+'\n')
