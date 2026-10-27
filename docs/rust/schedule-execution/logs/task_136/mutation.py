"""Demonstrate acceptance-test sensitivity; always restore temporary Rust mutations."""
import pathlib,subprocess,signal,json
signal.signal(signal.SIGTERM,lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))
r=pathlib.Path.cwd();folder=r/'docs/rust/schedule-execution/logs/task_136'
mutations=[('red-restore','pkg/schedule/scheduler_restore.rs','for (name, bytes) in loaded.unwrap_or_default() {','for (name, bytes) in loaded.unwrap_or_default().into_iter().take(0) {'),('red-cleanup','pkg/schedule/schedulers/scheduler_control.rs','pub async fn clean_registered(&mut self) {','pub async fn clean_registered(&mut self) {\n        return; // task136 temporary mutant: omit cleanup')]
for label,path,old,new in mutations:
 p=r/path;original=p.read_bytes();assert p.read_text().count(old)==1
 try:
  p.write_text(p.read_text().replace(old,new))
  child=subprocess.Popen(['python3',str(folder/'verify.py'),label,'cargo fmt --all','cargo test -p astersql-pd-compat-tests --locked --test schedule_restore -- --nocapture'])
  try: assert child.wait()==0
  except BaseException:
   child.terminate();child.wait();raise
  evidence=json.loads((folder/(label+'.json')).read_text())
  assert evidence[-1]['exit_code']==101,evidence
  assert any('FAILED' in x and 'failed' in x for x in evidence[-1]['test_summaries']),evidence
 finally:p.write_bytes(original)
print('Both production mutations rejected by behavior assertions; original bytes restored.')
