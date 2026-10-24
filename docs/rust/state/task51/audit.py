import hashlib, json, pathlib, re, subprocess
p=pathlib.Path(__file__).parent
before=json.loads((p/'before.json').read_text())
paths=subprocess.check_output(['git','ls-files','-co','--exclude-standard','-z']).decode().split('\0')
files={s:hashlib.sha256(pathlib.Path(s).read_bytes()).hexdigest() for s in sorted(set(paths)) if s and (s.endswith('.go') or pathlib.Path(s).name in ['go.mod','go.sum','go.work','go.work.sum']) and pathlib.Path(s).is_file()}
assert files==before['files']
status=subprocess.check_output(['git','status','--short','--','*.go','go.mod','go.sum','go.work','go.work.sum']).decode()
assert status==before['status']
for name,digest in before['plans'].items():
    assert hashlib.sha256(pathlib.Path(name).read_bytes()).hexdigest()==digest,name
result=dict(count=len(files),same_files_and_hashes=True,same_git_status=True,same_plans=True,aggregate_sha256=hashlib.sha256(json.dumps(files,sort_keys=True).encode()).hexdigest())
(p/'readonly.json').write_text(json.dumps(result,indent=2)+'\n')
for path in p.glob('*.out'):
    text='\n'.join(line.rstrip().expandtabs(4) for line in path.read_text().splitlines()).rstrip()
    path.write_text(text+'\n' if text else '')
subprocess.run(['git','diff','--check'],check=True)
subprocess.run(['git','diff','--cached','--check'],check=True)
slots=sorted(set(map(int,re.findall(r'SLOT=(\d+)', '\n'.join(f.read_text() for f in p.glob('*.out'))))))
resources=[]
ps=subprocess.check_output(['ps','-Ao','pid,ppid,stat,command'],text=True)
for n in slots:
    slot=pathlib.Path.cwd()/f'target/rust-slot-{n}'
    assert not (slot.parent/f'rust-slot-locks/slot-{n}.lock').exists()
    active=[line for line in ps.splitlines() if re.search(r'\s'+re.escape(str(slot))+r'/(?:rust-etcd-fixture|debug/(?:deps|examples))',line)]
    assert not active,active
    resources.append(dict(slot=n,CARGO_TARGET_DIR=str(slot),lock_released=True,cache_preserved=slot.is_dir(),active_slot_processes=active))
(p/'resources.json').write_text(json.dumps(resources,indent=2)+'\n')
for name in ['docs/rust/state/task-51.md','pkg/server/sync_transport.rs.md']:
    path=pathlib.Path(name)
    for link in re.findall(r'\]\(([^)]+)\)',path.read_text()):
        assert (path.parent/link.split('#')[0]).exists(),(name,link)
print(json.dumps(result)); print(json.dumps(resources))
