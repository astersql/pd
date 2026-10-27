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
(lock/'owner').write_text(f'plan=schedule-execution task=122 session=task-122 pid={os.getpid()}\n')
os.environ.update(CARGO_TARGET_DIR=str(root/f'target/rust-slot-{n}'),CARGO_BUILD_JOBS='4',RUST_TEST_THREADS='4',GOFLAGS='-mod=readonly')
folder=root/'docs/rust/schedule-execution/logs/task_122';result=[]
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
finally:
 (folder/f'{sys.argv[1]}.json').write_text(json.dumps(result,indent=2)+'\n');(lock/'owner').unlink();lock.rmdir()
