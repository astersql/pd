import pathlib,subprocess,json,hashlib,re,gzip
root=pathlib.Path.cwd();folder=root/'docs/rust/schedule-execution/logs/task_149'
base=json.loads((folder/'baseline.json').read_text())
files=subprocess.check_output(['rg','--files','--hidden','-g','!.git','-g','!target','-g','*.go','-g','go.mod','-g','go.sum','-g','go.work','-g','go.work.sum']).decode().splitlines()
plans=['.plans/2026-10-25-调度执行内核/plan.md','.plans/2026-10-25-调度执行内核/15-主题验收/plan.md']
now={p:hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest() for p in sorted(files+plans)}
assert now==base['files'],'Go/module/plan files changed'
status=subprocess.check_output(['git','status','--porcelain','--untracked-files=all','--','*.go','**/go.mod','**/go.sum','go.mod','go.sum','go.work','go.work.sum']).decode()
assert status==base['git_status']
prov=json.loads((folder/'provenance.json').read_text())
for p,h in prov['sha256'].items():assert hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()==h,p
for p in ['pkg/schedule/schedulers/shuffle_hot_region.go','pkg/schedule/schedulers/base_scheduler.go','pkg/schedule/schedulers/scheduler_test.go','pkg/utils/apiutil/apiutil.go']:
 assert subprocess.check_output(['git','show',prov['source_commit']+':'+p])==pathlib.Path(p).read_bytes()
for name in ['red','focused','focused2','final']:
 for entry in json.loads((folder/f'{name}.json').read_text()):
  assert hashlib.sha256((folder/entry['log']).read_bytes()).hexdigest()==entry['sha256']
final=json.loads((folder/'final.json').read_text());assert len(final)==7 and all(x['exit_code']==0 for x in final)
counts={}
for x in final:
 if 'test ' in x['command']:
  rows=[list(map(int,m)) for m in re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored',gzip.open(folder/x['log'],'rt').read())]
  counts[x['command']]=dict(zip(['passed','failed','ignored'],map(sum,zip(*rows))))
(folder/'final-summary.json').write_text(json.dumps(counts,indent=2)+'\n')
links=[]
for filename in ['docs/rust/schedule-execution/task-149.md','pkg/server/schedule_admin_shuffle_hot_region.rs.md']:
 p=pathlib.Path(filename)
 for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
  assert (p.parent/link).exists() or (p.parent/link).resolve()==(folder/"audit.json").resolve(),link
  links.append(link)
check=subprocess.run(['git','diff','--check'],capture_output=True,text=True);assert check.returncode==0,check.stdout
(folder/'audit.json').write_text(json.dumps(dict(protected_files=len(now),hashes_and_file_set_unchanged=True,go_module_git_status_unchanged=True,source_matches_commit=True,provenance_hashes_valid=True,log_hashes_valid=True,links_valid=links,diff_check_exit=check.returncode,counts=counts),indent=2)+'\n')
print(json.dumps(counts,indent=2))
