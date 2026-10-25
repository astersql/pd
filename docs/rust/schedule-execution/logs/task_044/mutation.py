"""Verify the frozen behavioral tests reject six distinct production regressions."""
from pathlib import Path
import os,subprocess,json
out=Path(__file__).resolve().parent
root=out.parents[4]
assert os.environ['CARGO_TARGET_DIR'].startswith(str(root/'target/rust-slot-'))
p=root/'pkg/schedule/checker/rule_fault.rs';original=p.read_text()
mutations=[
 ('down-boundary','>= ctx.replica.config.get_max_store_down_time()', '> ctx.replica.config.get_max_store_down_time()'),
 ('fast-failover','let fast = ctx.witness_enabled', 'let fast = false && ctx.witness_enabled'),
 ('promote-role','if p.is_learner() && rf.rule.role', 'if false && p.is_learner() && rf.rule.role'),
 ('cooldown','.is_some_and(|t| *t > ctx.replica.now_unix_nanos)', '.is_some_and(|t| *t < ctx.replica.now_unix_nanos)'),
 ('recorder-key','.keys()', '.values()'),
 ('priority','PriorityLevel::URGENT','PriorityLevel::HIGH'),
]
records=[]
try:
 for name,old,new in mutations:
  assert original.count(old)==1,(name,original.count(old))
  p.write_text(original.replace(old,new))
  cmd=['cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_044','--','--nocapture']
  with (out/f'mutation-{name}.log').open('w') as log:
   process=subprocess.Popen(cmd,cwd=root,stdout=log,stderr=subprocess.STDOUT)
   try:code=process.wait()
   except BaseException:
    process.terminate();process.wait();raise
  text=(out/f'mutation-{name}.log').read_text()
  assert code==101 and 'panicked at pkg/schedule/checker/task_044_test.rs' in text and 'error[E' not in text,(name,code)
  records.append(dict(name=name,command=cmd,exit_code=code,behavior_assertion=True))
  print(name,code,flush=True)
finally:
 p.write_text(original)
 (out/'mutations.json').write_text(json.dumps(records,indent=2)+'\n')
