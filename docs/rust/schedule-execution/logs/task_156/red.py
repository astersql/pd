"""Check acceptance sensitivity by mutating observed results and I/O boundary only."""
from pathlib import Path
import subprocess,gzip,signal
signal.signal(signal.SIGTERM,lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))
p=Path('tests/rust-compat/schedule_acceptance_test.rs');original=p.read_bytes()
f=Path('docs/rust/schedule-execution/logs/task_156')
try:
 s=original.decode().replace('assert_eq!(restored.encode_config().unwrap(), bytes);','assert_eq!({ let mut actual = restored.encode_config().unwrap(); actual.push(0); actual }, bytes);')
 s=s.replace('controller.pause_or_resume(&name, ttl, at).unwrap();','controller.pause_or_resume(&name, 0, at).unwrap();')
 s=s.replace('if self.fail.load(', 'if false && self.fail.load(')
 assert s != original.decode()
 p.write_text(s);gzip.open(f/'mutated-test.rs.gz','wb').write(p.read_bytes())
 child=subprocess.Popen(['python3',str(f/'verify.py'),'red','cargo fmt --all','cargo test -p astersql-pd-compat-tests --locked --test schedule_acceptance -- --nocapture'])
 try:
  child.wait()
 except BaseException:
  child.terminate();child.wait()  # runner waits for its Cargo children before releasing the slot
  raise
 assert child.returncode==101,child.returncode
finally:
 p.write_bytes(original)
