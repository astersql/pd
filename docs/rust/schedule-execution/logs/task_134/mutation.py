"""Prove the same fixed fixture rejects lost region identity, then restore."""
import pathlib,subprocess,json,signal
signal.signal(signal.SIGTERM, lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))
p=pathlib.Path('pkg/server/schedule_wire.rs');before=p.read_bytes()
try:
 p.write_bytes(before.replace(b'region_id: c.region_id,',b'region_id: 0,'))
 subprocess.run(['python3','docs/rust/schedule-execution/logs/task_134/verify.py','red','cargo fmt --all','cargo test -p astersql-pd-server --locked --lib task_134 -- --nocapture'],check=True)
 result=json.loads(pathlib.Path('docs/rust/schedule-execution/logs/task_134/red.json').read_text())[-1]
 assert result['exit_code']==101 and any('1 passed; 1 failed' in x for x in result['test_summaries']), result
finally:p.write_bytes(before)
