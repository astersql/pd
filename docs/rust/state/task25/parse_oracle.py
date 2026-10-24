"""Extract actual encoding/json bytes emitted by the existing Go binary."""
import json
from pathlib import Path
import re
s = Path(__file__).with_name('oracle-snapshots.out').read_text()
assert '/oracle/oracle.commands:' not in s
out = {}
for name, block in re.findall(r'"JSON (\w+)"\n(.*?)(?="JSON |\Z)', s, re.S):
    m = re.search(r'~r0: \[\]uint8 len: (\d+), cap: \d+, \[([\d,]+)\]', block)
    assert m, name
    b = bytes(map(int, m[2].split(',')))
    assert len(b) == int(m[1])
    out[name] = json.loads(b)
assert set(out) == {'collect', 'status', 'learner'}
fixture = Path('tests/rust-compat/fixtures/state/task-25.json')
assert json.loads(fixture.read_text())['snapshots'] == out
print('3 actual Go JSON snapshots match fixture; no debugger command errors')
