import os, pathlib, socket, subprocess, time, signal
def interrupted(signum, frame):
 raise SystemExit(128+signum)
signal.signal(signal.SIGINT, interrupted)
signal.signal(signal.SIGTERM, interrupted)
p=pathlib.Path(__file__).parent
slot=pathlib.Path(os.environ['CARGO_TARGET_DIR'])
def port():
 with socket.socket() as s:
  s.bind(('127.0.0.1',0)); return s.getsockname()[1]
c,e=port(),port(); url=f'http://127.0.0.1:{c}'; peer=f'http://127.0.0.1:{e}'
data=slot/f'oracle47-data-{os.getpid()}'
cmd=[str(slot/'pd-oracle47'),'--name','oracle47','--data-dir',str(data),'--client-urls',url,'--advertise-client-urls',url,'--peer-urls',peer,'--advertise-peer-urls',peer,'--initial-cluster',f'oracle47={peer}','--log-level','error']
print(cmd,flush=True)
with (p/'oracle-server.out').open('w') as log:
 child=subprocess.Popen(cmd,stdout=log,stderr=subprocess.STDOUT)
 try:
  import urllib.request
  for _ in range(120):
   if child.poll() is not None: raise RuntimeError(f'server exited {child.returncode}')
   try:
    with urllib.request.urlopen(url+'/pd/api/v1/health',timeout=1) as r:
     if b'true' in r.read(): break
   except Exception: pass
   time.sleep(.5)
  else: raise RuntimeError('server not ready')
  result=subprocess.run(['cargo','run','-p','astersql-pd-server','--locked','--example','store_heartbeat_oracle'],env={**os.environ,'PD_ORACLE_URL':url})
  raise SystemExit(result.returncode)
 finally:
  child.terminate()
  try: child.wait(timeout=20)
  except subprocess.TimeoutExpired: child.kill(); child.wait()
  import shutil
  shutil.rmtree(data,ignore_errors=True)
  print(f'Go server joined exit={child.returncode}; data removed',flush=True)
