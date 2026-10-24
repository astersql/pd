import hashlib, json, subprocess
from pathlib import Path
p=Path(__file__).parent
before=json.loads((p/'before.json').read_text())
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z']).decode().split('\0')
files={x:hashlib.sha256(Path(x).read_bytes()).hexdigest() for x in sorted(set(paths)) if x and (x.endswith('.go') or Path(x).name in ('go.mod','go.sum','go.work','go.work.sum')) and Path(x).is_file()}
status=subprocess.check_output(['git','status','--porcelain','--','*.go','go.mod','go.sum','go.work','go.work.sum']).decode()
plans={x:hashlib.sha256(Path(x).read_bytes()).hexdigest() for x in before['plans']}
changed=[x for x in sorted(set(before['files'])|set(files)) if files.get(x)!=before['files'].get(x)]
def aggregate(items):return hashlib.sha256(json.dumps(items,sort_keys=True).encode()).hexdigest()
result=dict(exit=int(bool(changed or status!=before['status'] or plans!=before['plans'])), file_count=len(files),before_sha256=aggregate(before['files']),after_sha256=aggregate(files),before_git_status=before['status'],after_git_status=status,plans=plans,changed_files=changed)
(p/'readonly.json').write_text(json.dumps(result,indent=2)+'\n')
print(result)
raise SystemExit(result['exit'])
