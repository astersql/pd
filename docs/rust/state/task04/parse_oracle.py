"""Extract observed snapshots; never compute rate expectations."""
import json
import pathlib
import re

root = pathlib.Path(__file__).resolve().parents[4]
source = pathlib.Path(__file__).with_name('oracle-snapshots.out').read_text()
assert '/oracle/commands:' not in source
snapshots = []
for name, block in re.findall(r'"CASE (\w+)"\n(.*?)(?="CASE |\Z)', source, re.S):
    period = {'original': 1000, 'zero': 0, 'truncate': 3,
              'wrap': 1, 'empty_staggered': 2000}[name]
    counters = [[1, 2, 3, 4] for _ in range(6)]
    if name == 'wrap':
        counters[0][0] = 18446744073709551615
    if name == 'empty_staggered':
        for dim in range(6):
            counters[dim][0] = 11 + dim
    keys = [[49], [50], [51], [52], [] if name == 'empty_staggered' else [53]]
    loads = [json.loads(value) for value in re.findall(r'Loads: [^\n]*?(\[[0-9,]+\])', block)]
    starts = [json.loads(value) for value in re.findall(r'StartKey: [^\n]*cap: \d+, (\[[0-9,]*\])', block)]
    ends = [json.loads(value) for value in re.findall(r'EndKey: [^\n]*cap: \d+, (\[[0-9,]*\])', block)]
    assert len(loads) == len(starts) == len(ends) == 4
    snapshots.append(dict(name=name, period=period, keys=keys, counters=counters,
                          interval=int(re.search(r'\n\s*interval: (\d+)', block)[1]),
                          loads=loads, starts=starts, ends=ends))
assert len(snapshots) == 5
out = root / 'tests/rust-compat/fixtures/state/task-04.json'
out.write_text(json.dumps(snapshots, indent=2) + '\n')
print('extracted 5 Go snapshots, 20 buckets, 120 observed rates')
