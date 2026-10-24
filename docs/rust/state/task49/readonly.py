import hashlib, json, pathlib, subprocess
p = pathlib.Path(__file__).parent
before = json.loads((p/'before.json').read_text())
files = subprocess.check_output(['git','ls-files','-z','--cached','--others','--exclude-standard']).decode().split('\0')
files = sorted(set(x for x in files if x and (x.endswith('.go') or pathlib.Path(x).name in ['go.mod','go.sum','go.work','go.work.sum'])))
hashes = {x: hashlib.sha256(pathlib.Path(x).read_bytes()).hexdigest() if pathlib.Path(x).exists() else None for x in files}
status = subprocess.check_output(['git','status','--porcelain','--','*.go','go.mod','go.sum','go.work','go.work.sum']).decode()
plans = {x: hashlib.sha256(pathlib.Path(x).read_bytes()).hexdigest() for x in before['plans']}
result = dict(count=len(files), same_files_and_hashes=hashes==before['files'], same_git_status=status==before['status'], same_plans=plans==before['plans'], aggregate_sha256=hashlib.sha256(json.dumps(hashes,sort_keys=True).encode()).hexdigest())
(p/'readonly.json').write_text(json.dumps(result,indent=2)+'\n')
assert result['same_files_and_hashes'] and result['same_git_status'] and result['same_plans'], result
print(result)
