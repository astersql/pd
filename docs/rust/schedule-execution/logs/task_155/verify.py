import pathlib,os,subprocess,gzip,json,sys,signal,hashlib,re,time
signal.signal(signal.SIGTERM, lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))
root=pathlib.Path.cwd();locks=root/'target/rust-slot-locks';locks.mkdir(parents=True,exist_ok=True)
while True:
 acquired=False
 for n in range(1,11):
  lock=locks/f'slot-{n}.lock'
  try:
   lock.mkdir();acquired=True;break
  except FileExistsError:pass
 if acquired:break
 time.sleep(1)
owner=f'plan=schedule-execution task=155 session=task-155 pid={os.getpid()}\n'
(lock/'owner').write_text(owner)
os.environ.update(CARGO_TARGET_DIR=str(root/f'target/rust-slot-{n}'),CARGO_BUILD_JOBS='4',RUST_TEST_THREADS='2',GOFLAGS='-mod=readonly')
folder=root/'docs/rust/schedule-execution/logs/task_155';result=[]
try:
 source=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'task155-existing-etcd'
 original=root/'target/rust-slot-1/rust-etcd-fixture-88178'
 assert hashlib.sha256(original.read_bytes()).hexdigest()=='57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5'
 if not source.exists(): os.link(original,source)
 # Existing binary provenance is task030; no rebuild, generation, failpoint injection or Go test.
 expected='57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5'
 os.environ.update(PD_EXISTING_ETCD_FIXTURE=str(source),PD_EXISTING_ETCD_FIXTURE_SHA256=expected,PD_GO_REUSE_LOG=str(folder/'go-reuse.jsonl'))
 os.environ['PATH']=str(folder/'reuse-go')+os.pathsep+os.environ['PATH']
 for i,cmd in enumerate(sys.argv[2:]):
  childenv=os.environ.copy()
  if '--test schedule_performance' in cmd:childenv['RUST_TEST_THREADS']='1'
  child=subprocess.Popen(cmd,shell=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,start_new_session=True,env=childenv)
  try:
   output=child.communicate()[0]
  except BaseException:
   os.killpg(child.pid,signal.SIGTERM)
   try: child.communicate(timeout=10)
   except subprocess.TimeoutExpired:
    os.killpg(child.pid,signal.SIGKILL);child.communicate()
   raise
  p=subprocess.CompletedProcess(cmd,child.returncode,output)
  path=folder/f'{sys.argv[1]}-{i}.log.gz';gzip.open(path,'wb').write(p.stdout)
  result.append(dict(command=cmd,exit_code=p.returncode,RUST_TEST_THREADS=int(childenv['RUST_TEST_THREADS']),slot=n,CARGO_TARGET_DIR=os.environ['CARGO_TARGET_DIR'],log=path.name,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),test_summaries=re.findall(r'test result:.*',p.stdout.decode(errors='replace'))))
  print(cmd,'exit',p.returncode,p.stdout.decode(errors='replace')[-1800:],flush=True)
  if p.returncode: break
finally:
 (folder/f'{sys.argv[1]}.json').write_text(json.dumps(result,indent=2)+'\n')
 if (lock/'owner').exists():
  if (lock/'owner').read_text()!=owner:
   raise RuntimeError('slot ownership changed; refusing to remove another owner')
  (lock/'owner').unlink();lock.rmdir()
 else:
  (folder/f'{sys.argv[1]}-cleanup.json').write_text(json.dumps({'slot':n,'owner_missing_at_cleanup':True,'children_finished':True})+'\n')

if result and result[-1]["exit_code"]:
 sys.exit(result[-1]["exit_code"])
