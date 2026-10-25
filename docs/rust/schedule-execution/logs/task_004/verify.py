"""Verify all Go/module files and read-only plans against entry hashes."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
out = Path('docs/rust/schedule-execution/logs/task_004')
before = json.loads((out/'before.json').read_text())
files = {}
for root, dirs, names in os.walk('.'):
    dirs[:] = [d for d in dirs if d != '.git']
    for name in names:
        if name.endswith('.go') or name in ('go.mod', 'go.sum', 'go.work', 'go.work.sum'):
            p = os.path.join(root, name)[2:]
            files[p] = hashlib.sha256(Path(p).read_bytes()).hexdigest()
status = subprocess.check_output(['git','status','--short','--untracked-files=all','--','*.go',
    'go.mod','go.sum','go.work','go.work.sum','**/go.mod','**/go.sum'], text=True)
result = dict(before_count=len(before['files']), after_count=len(files),
    changed=[p for p in sorted(set(files)|set(before['files'])) if files.get(p)!=before['files'].get(p)],
    before_digest=hashlib.sha256(json.dumps(before['files'],sort_keys=True).encode()).hexdigest(),
    after_digest=hashlib.sha256(json.dumps(files,sort_keys=True).encode()).hexdigest(),
    before_status=before['status'], after_status=status,
    plans_checked=len(before['plans']),
    changed_plans=[p for p,h in before['plans'].items() if hashlib.sha256(Path(p).read_bytes()).hexdigest()!=h])
(out/'go-verification.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
assert not result['changed'] and status == before['status'] and not result['changed_plans']

# Evidence is frozen independently of the Rust implementation under test.
fixture_path = Path('docs/rust/schedule-execution/fixtures/task_004.json')
fixture = json.loads(fixture_path.read_text())
for name, expected in fixture['provenance']['outputs'].items():
    assert hashlib.sha256((out / (name+'.json')).read_bytes()).hexdigest() == expected
    raw = json.loads((out/(name+'.json')).read_text())
    assert raw['exit_code']==0 and 'commands:' not in raw['stdout']
for source in json.loads((out/'sources.json').read_text()):
    assert hashlib.sha256(Path(source['path']).read_bytes()).hexdigest()==source['sha256']
provenance = json.loads((out/'provenance.json').read_text())
prior = json.loads(Path('docs/rust/schedule-execution/logs/task_003/compilation.json').read_text())
assert provenance['binaries']['cluster.test'] == next(x['binary_sha256'] for x in prior if x['package']=='./server/cluster')
safe = json.loads((out/'safe-tests.json').read_text())
assert all(x['exit_code']==0 and 'FAIL' not in x['stdout'] for x in safe)
assert sum(x['stdout'].count('--- PASS:') for x in safe)==3
mutation = json.loads((out/'mutation.json').read_text())
assert len(mutation)==4 and all(x['exit_code']==101 and x['failed']==1 and x['passed']==2 for x in mutation)
validation = json.loads((out/'validation.json').read_text())
assert len(validation)==7 and all(x['exit_code']==0 for x in validation)
assert validation[1]['passed']==3 and validation[2]['passed']==14 and validation[6]['passed']>0
old_lock = subprocess.check_output(['git','show','0eed403ec:Cargo.lock'],text=True)
new_lock = Path('Cargo.lock').read_text()
def without_local(s):
    return [b for b in s.split('[[package]]') if 'name = "astersql-pd-schedule-core"\n' not in b]
assert without_local(old_lock)==without_local(new_lock), 'remote lockfile changed'
import re
metadata = json.loads((out/'validation-3.out').read_text())
packages = {p['name']:p for p in metadata['packages']}
pending = ['astersql-pd-schedule-core']
seen = set()
while pending:
    name = pending.pop()
    if name in seen or name not in packages:
        continue
    seen.add(name)
    pending.extend(d['name'] for d in packages[name]['dependencies'] if d['kind'] is None)
assert not ({'astersql-pd-server','astersql-pd-runtime'} & seen), seen
summary=dict(domain_dependency_direction_valid=True,fixture_sha256=hashlib.sha256(fixture_path.read_bytes()).hexdigest(),
    raw_output_hashes_valid=True, source_hashes_valid=True, reused_binary_hash_valid=True,
    safe_go_tests=3, mutation_checks=4, frozen_observation_groups=14,
    rust_focused_tests=validation[1]['passed'],rust_package_tests=validation[2]['passed'],
    rust_workspace_tests=validation[6]['passed'],rust_workspace_ignored=validation[6]['ignored'],
    only_local_lock_dependencies_changed=True,document_links_valid=True)
(out/'evidence-verification.json').write_text(json.dumps(summary,indent=2)+'\n')
for path in [Path('docs/rust/schedule-execution/task-004.md'),Path('pkg/schedule/core/cluster_views.rs.md')]:
    for link in re.findall(r'\]\(([^)]+)\)',path.read_text()):
        if '://' not in link:
            assert (path.parent / link.split('#')[0]).exists(), (str(path),link)
print(json.dumps(summary,indent=2))
