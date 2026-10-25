"""Demonstrate registered tests reject wrong behavior, then restore production bytes."""
from pathlib import Path
import subprocess,json,os,hashlib
paths=[Path('pkg/schedule/operator/operator_record.rs'),Path('pkg/schedule/operator/operator.rs')]
before={p:p.read_bytes() for p in paths}
try:
 s=paths[0].read_text().replace('OpKind(k & k.wrapping_neg())','OpKind(0)').replace('duration_nanos: start.map(|s| signed_duration(finish_time, s)),','duration_nanos: start.map(|_| 0),').replace('        histories\n','        Vec::new()\n')
 assert s!=paths[0].read_text();paths[0].write_text(s)
 s=paths[1].read_text().replace('        self.status.to(CANCELED)\n','        self.status.to(CANCELED);\n        true\n');paths[1].write_text(s)
 subprocess.run(['cargo','fmt','--all'],check=True)
 argv=['cargo','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_025','--','--nocapture']
 p=subprocess.run(argv,capture_output=True,text=True)
 out=Path('docs/rust/schedule-execution/logs/task_025');(out/'red-behavior.out').write_text(p.stdout+p.stderr)
 (out/'red.json').write_text(json.dumps(dict(argv=argv,exit_code=p.returncode,cargo_target_dir=os.environ['CARGO_TARGET_DIR'],mutations=['scheduler low bit becomes zero','history becomes empty','record duration becomes zero','cancel always reports success']),indent=2)+'\n')
 assert p.returncode==101 and 'test result: FAILED' in p.stdout
 print(p.stdout[-1600:])
finally:
 for p,b in before.items():p.write_bytes(b)
 subprocess.run(['cargo','fmt','--all'],check=True)

assert all(p.read_bytes()==b for p,b in before.items())
report=json.loads((out/'red.json').read_text());report['restored_sha256']={str(p):hashlib.sha256(b).hexdigest() for p,b in before.items()};(out/'red.json').write_text(json.dumps(report,indent=2)+'\n')
