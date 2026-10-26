from pathlib import Path
import subprocess
p=Path('pkg/schedule/schedulers/hot_schedule.rs')
saved=p.read_bytes()
try:
 text=saved.decode()
 assert '&[HotResource::ReadLeader, HotResource::ReadPeer]' in text
 p.write_text(text.replace('&[HotResource::ReadLeader, HotResource::ReadPeer]','&[HotResource::ReadLeader]'))
 r=subprocess.run(['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_094','--','--nocapture'])
 assert r.returncode==101, f'expected behavioral failure, got {r.returncode}'
 print('Mutation rejected: read peer omitted; cargo exit 101')
finally:
 p.write_bytes(saved)
