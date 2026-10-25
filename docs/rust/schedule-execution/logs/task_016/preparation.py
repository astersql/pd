import hashlib,json,subprocess
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_016')
# Content and status of every tracked/untracked Go and module file; plans read-only.
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z']).decode().split('\0')
paths=sorted(set(p for p in paths if p and (p.endswith('.go') or Path(p).name in ['go.mod','go.sum','go.work','go.work.sum'])))
def snapshot():
 h=hashlib.sha256()
 for p in paths:
  h.update(p.encode()+b'\0'+(hashlib.sha256(Path(p).read_bytes()).digest() if Path(p).exists() else b'missing'))
 return dict(files=len(paths),sha256=h.hexdigest(),status=subprocess.check_output(['git','status','--porcelain','--','*.go','go.mod','go.sum','go.work','go.work.sum']).decode())
(out/'go-before.json').write_text(json.dumps(snapshot(),indent=2)+'\n')
plans=[Path('.plans/2026-10-25-调度执行内核/plan.md'),Path('.plans/2026-10-25-调度执行内核/04-操作器/plan.md')]
(out/'plans-before.json').write_text(json.dumps({str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in plans},indent=2)+'\n')
source=['pkg/schedule/operator/status_tracker.go','pkg/schedule/operator/status_tracker_test.go']
cases=[]
# Frozen source contract: only CREATED can expire, only STARTED can timeout;
# equality triggers a terminal transition; other states return terminal identity.
for status in range(7):
 for method,active,terminal in [('expired',0,5),('timeout',1,6)]:
  for elapsed in [9,10,11]:
   changed=status==active and elapsed>=10
   cases.append(dict(status=status,method=method,elapsed_ns=elapsed,duration_ns=10,result=changed or status==terminal,after=terminal if changed else status,changed=changed))
for method,terminal in [('expired',5),('timeout',6)]:
 cases.append(dict(status=0 if method=='expired' else 1,method=method,elapsed_ns=0,duration_ns=0,result=True,after=terminal,changed=True))
f=dict(evidence_kind='source_assertion',go_execution=False,source_commit='836fdc7cf3ef34f7eabc9d051418995bb97efabe',sources={p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in source},normalization='All times are explicit nonnegative nanosecond offsets from a monotonic test epoch. No state, boolean or timestamp differences ignored.',cases=cases)
Path('docs/rust/schedule-execution/fixtures/task_016.json').write_text(json.dumps(f,indent=2)+'\n')
# Reuse audited prior runner, changing task identity and paths only.
for name in ['slot-run.py','validation.py','readonly-validation.py']:
 text=Path('docs/rust/schedule-execution/logs/task_015',name).read_text().replace('015','016')
 if name=='readonly-validation.py':
  text=text.replace("source=target/'rust-etcd-fixture-2647'", "source=target/'rust-etcd-fixture-2647'")
 (out/name).write_text(text)
(out/'reuse-go').mkdir(exist_ok=True)
p=out/'reuse-go/go';p.write_text(Path('docs/rust/schedule-execution/logs/task_015/reuse-go/go').read_text().replace('015','016'));p.chmod(0o755)
