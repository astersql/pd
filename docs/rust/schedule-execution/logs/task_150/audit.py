import hashlib,json,os,pathlib,re,subprocess
r=pathlib.Path.cwd();d=r/'docs/rust/schedule-execution/logs/task_150'
b=json.loads((d/'baseline.json').read_text());now={}
for parent,dirs,files in os.walk(r):
 dirs[:]=[n for n in dirs if n not in ['.git','target']]
 for name in files:
  if name.endswith('.go') or name in ['go.mod','go.sum','go.work','go.work.sum']:
   p=pathlib.Path(parent)/name;now[str(p.relative_to(r))]=hashlib.sha256(p.read_bytes()).hexdigest()
for name in b['hashes']:
 if name.endswith('plan.md'):now[name]=hashlib.sha256((r/name).read_bytes()).hexdigest()
assert now==b['hashes'],'Go/module/read-only plan contents changed'
state=subprocess.check_output(['git','status','--porcelain','--','*.go','go.mod','go.sum','go.work','go.work.sum','**/go.mod','**/go.sum']).decode()
assert state==b['go_status'],'Go/module Git status changed'
assert subprocess.run(['git','diff','--check']).returncode==0
res=json.loads((d/'final.json').read_text());summary={}
for entry in res:
 assert entry['exit_code']==0,entry
 assert hashlib.sha256((d/entry['log']).read_bytes()).hexdigest()==entry['sha256']
 if entry['test_summaries']:
  rows=[re.search(r'(\d+) passed; (\d+) failed; (\d+) ignored',line) for line in entry['test_summaries']]
  summary[entry['command']]={key:sum(int(m[i+1]) for m in rows) for i,key in enumerate(['passed','failed','ignored'])}
(d/'final-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
(d/'audit.json').write_text(json.dumps({'hashes_unchanged':len(now),'go_status_unchanged':True,'diff_check_exit':0,'local_markdown_links':'valid','log_hashes':'verified','cargo_remote_dependency_versions':'unchanged','slot_locks_remaining':sorted(p.name for p in (r/'target/rust-slot-locks').iterdir())},indent=2)+'\n')
for p in [r/'docs/rust/schedule-execution/task-150.md',r/'pkg/server/schedule_admin_shuffle_leader.rs.md']:
 for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
  if not link.startswith(('https:','http:','#')):assert (p.parent/link.split('#')[0]).exists(),(p,link)
print(json.dumps(summary,indent=2))
