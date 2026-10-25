"""Prove source-contract tests detect actual wrong behavior; always restore source."""
import json,os,re,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_015');source=Path('pkg/schedule/operator/status.rs');original=source.read_text();rows=[]
changes=[
 ('terminal-reopens','if !self.current.permits(destination) {','if !self.current.permits(destination) && !(self.current.is_end() && destination == STARTED) {'),
 ('premature-timeout','if now.saturating_duration_since(reached) < duration {','if false && now.saturating_duration_since(reached) < duration {'),
 ('parse-witness','*name == flag && flag != "witness"','*name == flag'),
 ('merge-plus','value.bytes().all(|byte| byte.is_ascii_digit())','value.bytes().all(|byte| byte.is_ascii_digit() || byte == b\'+\')'),
 ('stale-reach-time','self.times[destination.0.min(2) as usize] = Some(now);','self.times[destination.0.min(2) as usize] = self.times[0];'),
]
try:
 for name,before,after in changes:
  assert original.count(before)==1,(name,before)
  source.write_text(original.replace(before,after))
  subprocess.run(['cargo','fmt','--all'],check=True)
  argv=['cargo','test','-p','astersql-pd-schedule-operator','--locked','--lib','task_015','--','--nocapture']
  p=subprocess.run(argv,capture_output=True,text=True);log=p.stdout+p.stderr;(out/f'red-{name}.out').write_text(log)
  assert p.returncode==101 and 'panicked at' in log and 'could not compile' not in log,(name,log)
  row=dict(mutation=name,argv=argv,exit_code=p.returncode,failed=int(re.search(r'(\d+) failed;',log)[1]),cargo_target_dir=os.environ['CARGO_TARGET_DIR']);rows.append(row);print(row,flush=True)
finally:
 source.write_text(original)
 subprocess.run(['cargo','fmt','--all'],check=True)
 (out/'mutation.json').write_text(json.dumps(rows,indent=2)+'\n')
