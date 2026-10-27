"""Audit Go/module immutability, fixed source hashes, plans and Rust-only scope."""
import os,pathlib,hashlib,json,subprocess,re
root=pathlib.Path.cwd();folder=root/'docs/rust/schedule-execution/logs/task_134'
def digest(p):return hashlib.sha256(p.read_bytes()).hexdigest()
files={}
for directory,dirs,names in os.walk(root):
 dirs[:]=[x for x in dirs if x not in ['.git','target']]
 for name in names:
  p=pathlib.Path(directory)/name
  if p.suffix=='.go' or name in ['go.mod','go.sum','go.work','go.work.sum']:files[str(p.relative_to(root))]=digest(p)
before=json.loads((folder/'readonly-before.json').read_text());assert files==before,'Go/module content or file set changed'
status=subprocess.check_output(['git','status','--porcelain','-z','--','*.go','go.mod','go.sum','go.work','go.work.sum'],text=True);assert not status,status
plans=['.plans/2026-10-25-调度执行内核/plan.md','.plans/2026-10-25-调度执行内核/15-主题验收/plan.md']
for p in plans:assert subprocess.check_output(['git','show',f'HEAD:{p}'])==(root/p).read_bytes(),p
proto=pathlib.Path('/Users/xiangmin/go/pkg/mod/github.com/pingcap/kvproto@v0.0.0-20261016101550-f8ad079c8e98')
sources=[proto/'proto'/f'{n}.proto' for n in ['pdpb','schedulingpb','metapb','encryptionpb']]+[proto/'pkg/pdpb/pdpb.pb.go']
provenance=json.loads((folder/'provenance.json').read_text());provenance['protocol_sources']={str(p.relative_to(proto)):digest(p) for p in sources}
provenance['artifacts']={p:digest(root/p) for p in ['docs/rust/schedule-execution/fixtures/task_012.json','docs/rust/schedule-execution/fixtures/task_134.json','docs/rust/schedule-execution/logs/task_134/freeze.py']}
(folder/'provenance.json').write_text(json.dumps(provenance,indent=2)+'\n')
subprocess.run(['git','diff','--check'],check=True)
for p in [root/'pkg/server/schedule_wire.rs.md',root/'docs/rust/schedule-execution/task-134.md']:
 for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
  if '://' not in link:
   target=p.parent/link.split('#')[0]
   if target.resolve()!=(folder/'audit.json').resolve():assert target.exists(),(p,link)
final=json.loads((folder/'final.json').read_text());assert all(x['exit_code']==0 for x in final)
assert not (root/'target/rust-slot-locks/slot-1.lock').exists()
(folder/'audit.json').write_text(json.dumps({'readonly_file_count':len(files),'hashes_and_file_set_equal':True,'go_git_status':status,'plans_equal_HEAD':plans,'diff_check_exit':0,'markdown_links':'passed'},indent=2)+'\n')
print('read-only files',len(files),'unchanged; plans, diff and links passed')
