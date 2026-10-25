"""Verify the frozen tests reject specific semantic errors; always restore source."""
from pathlib import Path
import subprocess,json,sys,signal

def interrupted(signum, frame):
 raise KeyboardInterrupt(f"signal {signum}")
signal.signal(signal.SIGTERM, interrupted)

def run(command, **kwargs):
 process=subprocess.Popen(command, **kwargs)
 try:
  return process.wait()
 except BaseException:
  process.terminate()
  process.wait()  # slot runner joins its Cargo process group before exiting
  raise
out=Path(__file__).resolve().parent
root=out.parents[4]
source=root/'pkg/schedule/checker/split_scatter_group.rs'
original=source.read_text()
mutations=[
 ('inclusive-upper','range.end_key > index_range.end_key','range.end_key >= index_range.end_key'),
 ('skip-validation','validate_keyspace.is_some_and(|validate| validate(id))','true'),
 ('malformed-index-fallback','let Ok((_, index_id)) = decode_int(index_rest) else {\n        return SplitScatterRangeHint::default();','let Ok((_, index_id)) = decode_int(index_rest) else {\n        return table_range();'),
 ('index-infinity','if range.end_key.is_empty()','if false'),
 ('prefix-no-truncation','let mut next = key[..=index].to_vec();','let mut next = key.to_vec();'),
]
results=[]
try:
 for name,old,new in mutations:
  assert original.count(old)==1,(name,original.count(old))
  source.write_text(original.replace(old,new))
  fmt=run([sys.executable,str(out/'run.py'),f'mutation-{name}-fmt','cargo','fmt','--all'],cwd=root,stdout=subprocess.DEVNULL)
  assert fmt==0
  result=run([sys.executable,str(out/'run.py'),f'mutation-{name}','cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_052','--','--nocapture'],cwd=root,stdout=subprocess.DEVNULL)
  log=(out/f'mutation-{name}.log').read_text()
  detected=result==101 and 'assertion `left == right` failed' in log
  results.append(dict(mutation=name,exit_code=result,behavior_assertion_detected=detected))
  print(results[-1],flush=True)
  assert detected
  source.write_text(original)
finally:
 source.write_text(original)
 (out/'mutation-results.json').write_text(json.dumps(results,indent=2)+'\n')
