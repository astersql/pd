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
(lock/'owner').write_text(f'plan=schedule-execution task=138 session=task-138 pid={os.getpid()}\n')
os.environ.update(CARGO_TARGET_DIR=str(root/f'target/rust-slot-{n}'),CARGO_BUILD_JOBS='4',RUST_TEST_THREADS='4',GOFLAGS='-mod=readonly')
folder=root/'docs/rust/schedule-execution/logs/task_138';result=[]
source=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'rust-etcd-fixture-2647'
# Existing binary provenance is task030; no rebuild, generation, failpoint injection or Go test.
expected='57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5'
os.environ.update(PD_EXISTING_ETCD_FIXTURE=str(source),PD_EXISTING_ETCD_FIXTURE_SHA256=expected,PD_GO_REUSE_LOG=str(folder/'go-reuse.jsonl'))
os.environ['PATH']=str(folder/'reuse-go')+os.pathsep+os.environ['PATH']

try:
 for i,cmd in enumerate(sys.argv[2:]):
  child=subprocess.Popen(cmd,shell=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,start_new_session=True)
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
  result.append(dict(command=cmd,exit_code=p.returncode,slot=n,CARGO_TARGET_DIR=os.environ['CARGO_TARGET_DIR'],log=path.name,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),test_summaries=re.findall(r'test result:.*',p.stdout.decode(errors='replace'))))
  print(cmd,'exit',p.returncode,p.stdout.decode(errors='replace')[-1800:],flush=True)
  if p.returncode: break
finally:
 (folder/f'{sys.argv[1]}.json').write_text(json.dumps(result,indent=2)+'\n');(lock/'owner').unlink();lock.rmdir()
