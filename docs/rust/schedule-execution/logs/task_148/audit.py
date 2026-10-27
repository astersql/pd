import hashlib,json,os,pathlib,re,subprocess
root=pathlib.Path.cwd();folder=root/'docs/rust/schedule-execution/logs/task_148'
base=json.loads((folder/'baseline.json').read_text());files={}
for current,dirs,names in os.walk(root):
 dirs[:]=[s for s in dirs if s not in ['.git','target']]
 for n in names:
  if n.endswith('.go') or n in ['go.mod','go.sum','go.work','go.work.sum']:
   p=pathlib.Path(current)/n;files[str(p.relative_to(root))]=hashlib.sha256(p.read_bytes()).hexdigest()
assert files==base['files'],'Go/module file set or contents changed'
status=subprocess.check_output(['git','status','--porcelain','--untracked-files=all']).decode()
def go_status(s):return [line for line in s.splitlines() if re.search(r'(\.go|go\.mod|go\.sum|go\.work|go\.work\.sum)"?$',line)]
assert go_status(status)==go_status(base['status'])
for p,h in base['plans'].items():assert hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()==h
markdown=['docs/rust/schedule-execution/task-148.md','pkg/server/schedule_admin_scatter_range.rs.md']
links=[]
for name in markdown:
 p=root/name
 for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
  target=p.parent/link
  assert target.exists() or target == folder/"audit.json",str(target)
  links.append(dict(source=name,target=link))
diff=subprocess.run(['git','diff','--check'],capture_output=True,text=True)
assert diff.returncode==0,diff.stdout+diff.stderr
assert not subprocess.check_output(['git','diff','--','Cargo.toml','Cargo.lock','pkg/server/Cargo.toml'])
for record in json.loads((folder/'final.json').read_text()):
 assert record['exit_code']==0
 assert hashlib.sha256((folder/record['log']).read_bytes()).hexdigest()==record['sha256']
 assert not (root/f"target/rust-slot-locks/slot-{record['slot']}.lock").exists()
(folder/'audit.json').write_text(json.dumps(dict(go_module_files=len(files),contents_and_set_unchanged=True,go_git_status_unchanged=True,readonly_plans_unchanged=True,cargo_manifests_and_lock_unchanged=True,markdown_links=links,diff_check_exit_code=diff.returncode,own_slot_released=True,reviewed_scope=['pkg/server/lib.rs module/test registration','pkg/server/schedule_admin_scatter_range.rs adapter','pkg/server/schedule_admin_scatter_range_test.rs behavior/fault/scheduling','fixture, provenance, logs and boundary evidence']),indent=2)+'\n')
print('audit passed:',len(files),'Go/module files unchanged; plans unchanged; links/log hashes/diff/slot cleanup verified')
