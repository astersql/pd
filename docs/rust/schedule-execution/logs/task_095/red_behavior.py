from pathlib import Path
import subprocess
p=Path('pkg/schedule/schedulers/hot_pending.rs')
saved=p.read_bytes()
try:
 text=saved.decode()
 needle='                    origin,\n'
 assert text.count(needle)==1
 p.write_text(text.replace(needle,'                    origin: Influence { loads: vec![0.; 8], hot_peer_count: 0. },\n'))
 r=subprocess.run(['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_095_behavior_cases','--','--nocapture'])
 assert r.returncode==101, f'expected behavioral failure, got {r.returncode}'
 print('Mutation rejected by lifecycle assertion: zeroed admitted influence; cargo exit 101')
finally:
 p.write_bytes(saved)
