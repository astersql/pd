"""Prove the same integration assertions detect a missing success notification."""
import hashlib, json, pathlib, subprocess, sys
root=pathlib.Path.cwd()
p=root/'pkg/schedule/operator/controller_dispatch.rs'; original=p.read_bytes()
needle=b'out.completed = Some(op.clone());'
assert original.count(needle)==1
try:
 p.write_bytes(original.replace(needle,b'out.completed = None; // task135 temporary mutation'))
 subprocess.run([sys.executable,'docs/rust/schedule-execution/logs/task_135/verify.py','red',
  'cargo test -p astersql-pd-compat-tests --locked --test schedule_loop -- --nocapture'],check=True)
 result=json.loads(pathlib.Path('docs/rust/schedule-execution/logs/task_135/red.json').read_text())
 assert result[0]['exit_code']==101,'mutation unexpectedly survived'
finally:
 p.write_bytes(original)
 assert hashlib.sha256(p.read_bytes()).digest()==hashlib.sha256(original).digest()
