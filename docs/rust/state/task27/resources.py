from pathlib import Path
import subprocess
root=Path.cwd()
slot=root/'target/rust-slot-1'
assert slot.is_dir(), 'slot cache must remain'
assert not (root/'target/rust-slot-locks/slot-1.lock').exists(), 'own lock remains'
rows=subprocess.check_output(['ps','-Ao','pid,ppid,args'],text=True).splitlines()
active=[]
for row in rows[1:]:
 parts=row.split(None,2)
 if len(parts)!=3:continue
 command=parts[2]
 if command.startswith(str(slot)) or (command.startswith('go build ') and str(slot) in command) or (command.startswith('docker run ') and str(slot) in command):
  active.append(row)
assert not active, active
print('slot 1: lock released; cache retained; no child uses slot artifacts')
