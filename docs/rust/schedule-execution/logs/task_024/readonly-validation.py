"""Run all Rust baselines with no Go compilation, even inside existing fixtures."""
import hashlib,json,os,runpy
from pathlib import Path
out=Path('docs/rust/schedule-execution/logs/task_024').resolve()
target=Path(os.environ['CARGO_TARGET_DIR']);source=target/'rust-etcd-fixture-2647'
assert source.is_file(),'preexisting task014 etcd fixture required in the claimed slot'
os.environ['PD_EXISTING_ETCD_FIXTURE']=str(source)
os.environ['PD_EXISTING_ETCD_FIXTURE_SHA256']=hashlib.sha256(source.read_bytes()).hexdigest()
os.environ['PD_GO_REUSE_LOG']=str(out/'go-reuse.jsonl')
os.environ['PATH']=str(out/'reuse-go')+os.pathsep+os.environ['PATH']
(out/'fixture-reuse-provenance.json').write_text(json.dumps(dict(binary=str(source),binary_sha256=os.environ['PD_EXISTING_ETCD_FIXTURE_SHA256'],source='scripts/rust-etcd-fixture/main.go',source_sha256=hashlib.sha256(Path('scripts/rust-etcd-fixture/main.go').read_bytes()).hexdigest(),binary_vcs_revision='e064eee5ca1a50c6bb917855e425e87e31ad5ebe',scope='existing external etcd I/O fixture only; no operator oracle; no Go compilation'),indent=2)+'\n')
runpy.run_path(str(out/'validation.py'),run_name='__main__')
