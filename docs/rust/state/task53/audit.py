import hashlib,json,pathlib,re,subprocess
p=pathlib.Path(__file__).parent
before=json.loads((p/'before.json').read_text())
paths=subprocess.check_output(['git','ls-files','-co','--exclude-standard','-z']).decode().split('\0')
files={s:hashlib.sha256(pathlib.Path(s).read_bytes()).hexdigest() for s in sorted(set(paths)) if s and (s.endswith('.go') or pathlib.Path(s).name in ['go.mod','go.sum','go.work','go.work.sum']) and pathlib.Path(s).is_file()}
assert files==before['files']
assert subprocess.check_output(['git','status','--short','--','*.go','go.mod','go.sum','go.work','go.work.sum'],text=True)==before['status']
assert all(hashlib.sha256(pathlib.Path(n).read_bytes()).hexdigest()==h for n,h in before['plans'].items())
result=dict(count=len(files),same_files_and_hashes=True,same_git_status=True,same_plans=True,aggregate_sha256=hashlib.sha256(json.dumps(files,sort_keys=True).encode()).hexdigest())
(p/'readonly.json').write_text(json.dumps(result,indent=2)+'\n')
for path in p.glob('*.out'):
 s='\n'.join(line.rstrip().expandtabs(4) for line in path.read_text().splitlines()).rstrip()
 path.write_text(s+'\n' if s else '')
ps=subprocess.check_output(['ps','-Ao','pid,ppid,stat,command'],text=True)
slot=pathlib.Path.cwd()/'target/rust-slot-2'
assert not (slot.parent/'rust-slot-locks/slot-2.lock').exists()
active=[line for line in ps.splitlines() if re.search(r'\s'+re.escape(str(slot))+r'/(?:rust-etcd-fixture|debug/(?:deps|examples)|pd-oracle47.*--name oracle53)',line)]
assert not active,active
assert not subprocess.check_output(['docker','ps','-a','--filter','name=pd-state53','--format','{{.Names}}'],text=True).strip()
# Only retain directories whose endpoint appears in this task's own fixture logs,
# created after its starting audit, and whose owning PID has exited.
removed=json.loads((p/'resources.json').read_text()).get('removed_own_failed_fixture_data',[]) if (p/'resources.json').exists() else []
endpoints=set(re.findall(r'isolated etcd 3.5.15 ready at (http://[^\s]+)', '\n'.join(f.read_text() for f in p.glob('*.out'))))
for d in slot.glob('etcd-contract-*'):
 if d.stat().st_mtime >= (p/'before.json').stat().st_mtime and (d/'ready').exists() and (d/'ready').read_text().strip() in endpoints:
  pid=d.name.split('-')[2]
  if subprocess.run(['ps','-p',pid],stdout=subprocess.DEVNULL).returncode!=0:
   import shutil
   removed.append(d.name);shutil.rmtree(d)
resources=dict(slot=2,CARGO_TARGET_DIR=str(slot),lock_released=True,cache_preserved=slot.is_dir(),active_slot_processes=active,removed_own_failed_fixture_data=removed,oracle_data_removed=not list(slot.glob('oracle53-data-*')))
assert resources['oracle_data_removed']
(p/'resources.json').write_text(json.dumps(resources,indent=2)+'\n')
for name in ['docs/rust/state/task-53.md','tests/rust-compat/state_regression.rs.md','pkg/core/region_inherit.rs.md']:
 path=pathlib.Path(name)
 for link in re.findall(r'\]\(([^)]+)\)',path.read_text()):
  assert (path.parent/link.split('#')[0]).exists(),(name,link)
subprocess.run(['git','diff','--check'],check=True)
print(json.dumps(result));print(json.dumps(resources))
