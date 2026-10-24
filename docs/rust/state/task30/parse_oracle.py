import json, re
from pathlib import Path
source = Path('docs/rust/state/task30/snapshot.out').read_text()
values = [int(x) for x in re.findall(r'^([0-9]+)$', source, re.M)]
assert len(values) == 6 and 'PASS' in source and 'exited with status 0' in source
n, head, tail, size, a, b = values
fixture = dict(source='history_buffer_test.go:50-64, dlv h.index/head/tail/size and records meta.Id',
               baseline='69ba16e6d47dfe59a21498c21641e1c1eaa726e0',
               reset=100, input=[1,2,3], capacity=size-1, next=n,
               first=n-((tail-head)%size), records=[a,b])
Path('tests/rust-compat/fixtures/state/task-30.json').write_text(json.dumps(fixture, indent=2)+'\n')
