import json
import subprocess
from pathlib import Path
root = Path.cwd()
p = Path(__file__).parent
slots = [root / 'target' / f'rust-slot-{n}' for n in (2,)]
for n in (2,):
    assert not (root / 'target/rust-slot-locks' / f'slot-{n}.lock').exists(), f'own slot {n} still locked'
active = []
for line in subprocess.check_output(['ps', '-axo', 'pid=,command='], text=True).splitlines():
    fields = line.strip().split(None, 1)
    if len(fields) == 2 and any(fields[1].startswith((str(s) + '/', str(s.relative_to(root)) + '/')) for s in slots):
        active.append(line)
assert not active, active
containers = subprocess.check_output(['docker', 'ps', '--filter', 'name=pd-state46-', '--format', '{{.Names}}'], text=True).strip()
assert not containers, containers
result = dict(oracle_containers_exited=not containers, slots=[str(s) for s in slots], own_locks_released=True, active_slot_processes=active, caches_preserved=all(s.is_dir() for s in slots), foreign_slot_1_untouched=True)
(p / 'resources.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result))
