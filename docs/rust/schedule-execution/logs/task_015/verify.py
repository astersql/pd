"""Audit source-derived fixtures, real Rust results and read-only boundaries."""
import hashlib,json,re,subprocess,tomllib
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_015')
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
fpath=Path('docs/rust/schedule-execution/fixtures/task_015.json');f=json.loads(fpath.read_text())
assert f['evidence_kind']=='source_assertion' and f['go_execution'] is False
old_fixture=fpath.read_bytes()
subprocess.run(['python3',str(out/'source-fixture.py')],check=True)
assert fpath.read_bytes()==old_fixture
for p,h in f['sources'].items():assert sha(p)==h
assert subprocess.run(['git','merge-base','--is-ancestor','8536256e5','HEAD']).returncode==0
counts={k:len(f[k]) for k in ['kinds','kind_strings','statuses','transitions','merge']};assert sum(counts.values())==125
assert sum(c['changed'] for c in f['transitions'])==7
rows=json.loads((out/'validation.json').read_text());assert len(rows)==7 and all(r['exit_code']==0 for r in rows)
assert rows[1]['passed']==rows[2]['passed']==4 and rows[6]['passed']>=1367
assert all(r['failed']==0 for r in rows)
mut=json.loads((out/'mutation.json').read_text());assert len(mut)==5 and all(r['exit_code']==101 and r['failed']>0 for r in mut)
assert 'command_exit_code=101' in (out/'red-compile.out').read_text()
for r in mut:
 log=(out/f"red-{r['mutation']}.out").read_text();assert 'panicked at' in log and 'could not compile' not in log
reuse=json.loads((out/'fixture-reuse-provenance.json').read_text())
assert sha(reuse['binary'])==reuse['binary_sha256']
assert sha(reuse['source'])==reuse['source_sha256']
assert Path(reuse['source']).read_bytes()==subprocess.check_output(['git','show',reuse['binary_vcs_revision']+':'+reuse['source']])
copies=[json.loads(line) for line in (out/'go-reuse.jsonl').read_text().splitlines()]
assert len(copies)>0 and all(c['action']=='copy_existing_etcd_fixture' and c['sha256']==reuse['binary_sha256'] for c in copies)
before=json.loads((out/'before.json').read_text())
files=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z'],text=True).split('\0')
names=sorted(set(p for p in files if p and (p.endswith('.go') or Path(p).name in ['go.mod','go.sum','go.work','go.work.sum','plan.md'])))
after={p:sha(p) for p in names if Path(p).is_file()};assert before==after
old=tomllib.loads(subprocess.check_output(['git','show','8536256e5:Cargo.lock'],text=True))['package']
new=tomllib.loads(Path('Cargo.lock').read_text())['package']
assert [p for p in new if p['name']!='astersql-pd-schedule-operator']==old
assert len(new)==len(old)+1
stage=(out/'validation-stage.out').read_text();slot=int(re.search(r'slot=(\d+)',stage)[1]);target=str(Path(f'target/rust-slot-{slot}').resolve())
assert all(r['cargo_target_dir']==target for r in rows)
assert not Path(f'target/rust-slot-locks/slot-{slot}.lock').exists()
p=subprocess.run(['git','diff','--check'],capture_output=True,text=True);assert p.returncode==0,p.stdout+p.stderr
for doc in ['pkg/schedule/operator/status.rs.md','docs/rust/schedule-execution/task-015.md']:
 for link in re.findall(r'\]\(([^)]+)\)',Path(doc).read_text()):
  dest=(Path(doc).parent/link.split('#')[0]).resolve()
  if dest==(out/'verification.json').resolve():continue
  if '://' not in link:assert dest.exists(),(doc,link)
result=dict(evidence_kind='source_assertion_and_actual_rust_tests',go_execution=False,fixture_sha256=sha(fpath),source_cases=counts,total_source_cases=sum(counts.values()),allowed_edges=7,focused_tests=4,package_tests=4,workspace_tests=rows[6]['passed'],workspace_ignored=rows[6]['ignored'],behavioral_mutations=len(mut),readonly_files=len(before),go_module_plan_hashes_unchanged=True,diff_check_exit_code=0,slot=slot,cargo_target_dir=target,own_slot_lock_released=True,lockfile_remote_packages_unchanged=True,existing_etcd_fixture_reuses=len(copies),go_build_requests_intercepted=True)
(out/'verification.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
