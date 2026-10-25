"""Behavioral red evidence: restore the canonical source after every mutation."""
from pathlib import Path
import subprocess,json,re,os
out=Path('docs/rust/schedule-execution/logs/task_014')
paths={name:Path('pkg/schedule/hbstream/'+name+'.rs') for name in ['routing','stream_lifecycle']}
originals={name:p.read_text() for name,p in paths.items()};rows=[]
mutations=[
 ('early_tick','stream_lifecycle','Duration::from_secs(60)','Duration::from_secs(30)'),
 ('wrong_cluster','routing','self.streams[&id].keep_alive(cluster_id)','self.streams[&id].keep_alive(cluster_id+1)'),
 ('failed_keepalive_retained','routing','if result.is_err() {','if false && result.is_err() {'),
 ('buried_metrics_recreated','routing','if self.stores.get_store(id).is_some_and(|s| !s.is_removed()) {','if true {'),
 ('skip_not_counted','routing','self.record_metrics(id, store.address(), "push", "skip");',''),
 ('cancel_reports_success','routing','_ = self.cancel.cancelled() => Err(mpsc::error::SendError(command)),','_ = self.cancel.cancelled() => Ok(()),'),
 ('join_aborts_io','stream_lifecycle','let result = task.await;','task.abort(); let result = task.await;'),
]
try:
 for name,file,before,after in mutations:
  p=paths[file];original=originals[file]
  assert before in original,name
  p.write_text(original.replace(before,after))
  fmt=subprocess.run(['cargo','fmt','--all'],capture_output=True,text=True);assert fmt.returncode==0
  argv=['cargo','test','-p','astersql-pd-schedule-hbstream','--locked','--lib','task_014','--','--nocapture']
  proc=subprocess.run(argv,capture_output=True,text=True,timeout=90)
  log=proc.stdout+proc.stderr;(out/f'red-{name}.out').write_text(log)
  assert proc.returncode==101 and 'panicked at' in log and 'test result: FAILED' in log and 'could not compile' not in log,name
  row=dict(mutation=name,argv=argv,exit_code=proc.returncode,failed=int(re.search(r'(\d+) failed;',log)[1]),cargo_target_dir=os.environ['CARGO_TARGET_DIR']);rows.append(row);print(row,flush=True)
  (out/'mutation.json').write_text(json.dumps(rows,indent=2)+'\n')
  p.write_text(original)
finally:
 for name,p in paths.items():p.write_text(originals[name])
 subprocess.run(['cargo','fmt','--all'],check=True)
