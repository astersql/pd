from pathlib import Path
import subprocess,signal,gzip,json
signal.signal(signal.SIGTERM,lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))
p=Path('pkg/storage/endpoint/endpoint_codec.rs');original=p.read_bytes();f=Path('docs/rust/schedule-execution/logs/task_156')
try:
 s=original.decode().replace('domain: astersql_pd_storage_domain::StorageEndpoint::new(store.clone()),','domain: astersql_pd_storage_domain::StorageEndpoint::new(Arc::new(astersql_pd_storage_kv::MemoryStore::default())),')
 assert s!=original.decode();p.write_text(s);gzip.open(f/'split-mutated-adapter.rs.gz','wb').write(p.read_bytes())
 child=subprocess.Popen(['python3',str(f/'verify.py'),'split-red','cargo fmt --all','cargo test -p astersql-pd-compat-tests --locked --test schedule_acceptance -- --nocapture'])
 try:child.wait()
 except BaseException:child.terminate();child.wait();raise
 assert child.returncode==101,child.returncode
finally:p.write_bytes(original)
