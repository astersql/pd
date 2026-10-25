"""Prove registered behavior assertions detect loss; restore every mutation."""
import subprocess, json, re, os
from pathlib import Path
out=Path(__file__).parent
source=Path('pkg/schedule/hbstream/operation.rs')
saved=source.read_text()
command=['cargo','test','-p','astersql-pd-schedule-hbstream','--locked','--lib','task_012','--','--nocapture']
mutations=[]
for field in ['change_peer','transfer_leader','merge','split_region','change_peer_v2','switch_witnesses','change_split']:
    mutations.append(('drop_'+field, 'let target_peer = region.leader()?.clone();', 'let target_peer = region.leader()?.clone();\nlet mut operation=operation; operation.'+field+'=None;'))
mutations += [('drop_epoch','region_epoch: region.meta().epoch,','region_epoch: None,'),('wrong_leader','let target_peer = region.leader()?.clone();','let mut target_peer = region.leader()?.clone(); target_peer.store_id=0;'),('wrong_cluster','            cluster_id,','            cluster_id: 0,'),('false_split','let target_peer = region.leader()?.clone();','let target_peer = region.leader()?.clone();\nlet mut operation=operation; if let Some(s)=operation.change_split.as_mut() { s.auto_split_enabled=false; }'),('sort_keys','let target_peer = region.leader()?.clone();','let target_peer = region.leader()?.clone();\nlet mut operation=operation; if let Some(s)=operation.split_region.as_mut() { s.keys.sort(); }')]
results=[]
try:
    for name,before,after in mutations:
        assert saved.count(before)==1,(name,before)
        source.write_text(saved.replace(before,after))
        subprocess.run(['cargo','fmt','--all'],check=True)
        p=subprocess.run(command,capture_output=True,text=True)
        log=p.stdout+p.stderr;(out/(name+'.out')).write_text(log)
        failed=re.search(r'test result: FAILED\. (\d+) passed; (\d+) failed',log)
        row=dict(mutation=name,argv=command,exit_code=p.returncode,failed=int(failed[2]) if failed else 0,cargo_target_dir=os.environ['CARGO_TARGET_DIR'])
        results.append(row);(out/'mutation.json').write_text(json.dumps(results,indent=2)+'\n');print(row,flush=True)
        assert p.returncode==101 and failed and int(failed[2])>0 and 'assertion `left == right` failed' in log,log
finally:
    source.write_text(saved)
    subprocess.run(['cargo','fmt','--all'],check=True)
