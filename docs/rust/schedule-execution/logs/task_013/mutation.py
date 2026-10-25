"""Behavioral red evidence: restore the canonical source after every mutation."""
from pathlib import Path
import subprocess,json,re,os
out=Path('docs/rust/schedule-execution/logs/task_013')
p=Path('pkg/schedule/hbstream/routing.rs');original=p.read_text();rows=[]
mutations=[
 ('stale_binding','self.streams.insert(update.store_id, update.stream);','self.streams.entry(update.store_id).or_insert(update.stream);'),
 ('wrong_target','let id = command.target_peer.store_id;','let id = 1;'),
 ('removed_allowed','if store.is_removed() {','if false && store.is_removed() {'),
 ('missing_not_deleted','self.streams.remove(&id);\n            return RouteEvent::MissingStore(id);','return RouteEvent::MissingStore(id);'),
 ('failed_not_deleted','self.streams.remove(&id);\n                RouteEvent::SendFailed','RouteEvent::SendFailed'),
 ('payload_corrupt','match stream.send(command).await {','let mut command = command; command.region_id = 0;\n        match stream.send(command).await {'),
 ('capacity_wrong','HEARTBEAT_CHANNEL_CAPACITY: usize = 1024','HEARTBEAT_CHANNEL_CAPACITY: usize = 1025'),
]
try:
 for name,before,after in mutations:
  assert before in original,name
  p.write_text(original.replace(before,after))
  fmt=subprocess.run(['cargo','fmt','--all'],capture_output=True,text=True);assert fmt.returncode==0
  argv=['cargo','test','-p','astersql-pd-schedule-hbstream','--locked','--lib','task_013','--','--nocapture']
  proc=subprocess.run(argv,capture_output=True,text=True,timeout=90)
  log=proc.stdout+proc.stderr;(out/f'red-{name}.out').write_text(log)
  assert proc.returncode==101 and 'panicked at' in log and 'test result: FAILED' in log and 'could not compile' not in log,name
  row=dict(mutation=name,argv=argv,exit_code=proc.returncode,failed=int(re.search(r'(\d+) failed;',log)[1]),cargo_target_dir=os.environ['CARGO_TARGET_DIR']);rows.append(row);print(row,flush=True)
  (out/'mutation.json').write_text(json.dumps(rows,indent=2)+'\n')
finally:
 p.write_text(original)
 subprocess.run(['cargo','fmt','--all'],check=True)
