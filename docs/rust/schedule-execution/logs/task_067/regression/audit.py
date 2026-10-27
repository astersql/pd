"""Verify task067 regression evidence and strict Go/plan read-only boundaries."""
from pathlib import Path
import hashlib, json, os, re, gzip, subprocess
root=Path.cwd(); folder=root/'docs/rust/schedule-execution/logs/task_067/regression'
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
before=json.loads((folder/'before.json').read_text())
actual={}
for base,dirs,files in os.walk(root):
    dirs[:]=[d for d in dirs if d not in ['.git','target','node_modules']]
    for n in files:
        p=Path(base)/n
        if p.suffix=='.go' or n in ['go.mod','go.sum','go.work','go.work.sum']:
            actual[str(p.relative_to(root))]=sha(p)
assert actual==before['go'],'Go or module source content/set changed'
status=subprocess.check_output(['git','status','--porcelain','--','*.go','go.mod','go.sum','go.work','go.work.sum']).decode()
assert status==before['go_status']
assert all(sha(root/p)==h for p,h in before['plans'].items())
assert all(sha(root/p)==h for p,h in json.loads((folder/'current-inputs.json').read_text()).items())
rows=json.loads((folder/'final.json').read_text()); summaries=[]
assert len(rows)==6
for r in rows:
    assert r['exit_code']==0,r
    assert sha(folder/r['log'])==r['sha256']
    assert r['CARGO_TARGET_DIR']==str(root/f"target/rust-slot-{r['slot']}")
    content=gzip.open(folder/r['log'],'rt').read()
    counts=[tuple(map(int,x)) for x in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',content)]
    summaries.append(dict(command=r['command'],exit_code=0,passed=sum(x[0] for x in counts),failed=sum(x[1] for x in counts),ignored=sum(x[2] for x in counts)))
assert summaries[0]['passed']==3
assert summaries[1]['passed']==192
assert summaries[-1]['passed']==1925 and summaries[-1]['failed']==0 and summaries[-1]['ignored']==1
for r in rows:
    assert not (root/f"target/rust-slot-locks/slot-{r['slot']}.lock").exists(),'own slot still held'
requests=[json.loads(line) for line in (folder/'go-reuse.jsonl').read_text().splitlines()]
assert requests and all(r['action'] in ['atomic_hardlink_existing_etcd_fixture','readonly_cached_kvproto_zip'] for r in requests)
assert all(r['sha256']=='57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5' for r in requests if r['action']=='atomic_hardlink_existing_etcd_fixture')
subprocess.run(['git','diff','--check'],check=True)
report=dict(go_files=len(actual),go_files_and_status_unchanged=True,plans_unchanged=True,rust_validation_inputs_unchanged=True,own_slot_released=True,go_reuse_requests=len(requests),commands=summaries)
(folder/'audit.json').write_text(json.dumps(report,indent=2)+'\n')
for p in [root/'docs/rust/schedule-execution/task-067.md',root/'pkg/schedule/schedulers/balance_solver.rs.md']:
    for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
        assert (p.parent/link).exists(),(p,link)
print(json.dumps(report,indent=2))
