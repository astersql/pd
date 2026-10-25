"""Prove the behavior tests reject TTL, retry, attempt and ordering defects."""
from pathlib import Path
import subprocess,sys,json
root=Path(__file__).resolve().parents[5]
out=Path(__file__).resolve().parent
source=root/'pkg/schedule/checker/split_scatter_retry.rs'
original=source.read_text()
mutations=[
 ('ttl-exclusive','now >= p.expire_at','now > p.expire_at'),
 ('no-backoff','now + SPLIT_SCATTER_RETRY_BACKOFF;','now;'),
 ('never-attempted','.attempted = true;','.attempted = false;'),
 ('reverse-expiry','.cmp(&b.expire_at)','.cmp(&b.expire_at).reverse()'),
]
results=[]
try:
 for name,old,new in mutations:
  assert old in original,old
  source.write_text(original.replace(old,new,1))
  subprocess.run([sys.executable,str(out/'run.py'),f'mutation-{name}-fmt','cargo','fmt','--all'],cwd=root,check=True,stdout=subprocess.DEVNULL)
  proc=subprocess.run([sys.executable,str(out/'run.py'),f'mutation-{name}','cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_054','--','--nocapture'],cwd=root,stdout=subprocess.DEVNULL)
  log=(out/f'mutation-{name}.log').read_text()
  results.append(dict(mutation=name,exit_code=proc.returncode,behavior_assertion_failed='assertion' in log and 'FAILED' in log))
  assert proc.returncode==101 and results[-1]['behavior_assertion_failed'],results[-1]
finally:
 source.write_text(original)
 (out/'mutation-results.json').write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps(results))
