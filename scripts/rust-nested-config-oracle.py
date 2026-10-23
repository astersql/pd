#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
# Captures only an existing Go binary; never builds or modifies Go sources.
from pathlib import Path
import subprocess, tempfile, json, hashlib, time, selectors
root=Path(__file__).resolve().parents[1]; binary=root/'bin/pd-server'
# This existing binary's Parse/Adjust path does not execute generators or source writes.
valid={
'missing':'',
'cased':'[SCHEDULE]\nMAX-SNAPSHOT-COUNT=0\nENABLE-JOINT-CONSENSUS=false\n',
'unknown':'[schedule]\nunknown-field=false\n',
'explicit':'''[schedule]
max-snapshot-count = 0
max-pending-peer-count = 3
max-store-preparing-time = "0s"
enable-joint-consensus = false
enable-cross-table-merge = false
region-score-formula-version = "v1"
store-balance-rate = 7.5
disable-remove-down-replica = true
[replication]
max-replicas = 0
location-labels = ["zone", "$rack/a"]
isolation-level = "zone"
enable-placement-rules = false
[controller]
ltb-max-wait-duration = "0s"
ltb-token-rpc-max-delay = "-1s"
push-metrics-address = "http://metrics.invalid"
push-metrics-interval = "0s"
enable-controller-trace-log = true
[controller.request-unit]
read-base-cost = 0.0
write-base-cost = -2.0
[controller.ru-version-policy]
default = 2
[security]
redact-info-log = "marker"
cert-allowed-cn = ["PD"]
SSLCABytes = [97, 98, 99]
[security.encryption]
data-encryption-method = "aes256-ctr"
data-key-rotation-period = "2h"
[security.encryption.master-key]
type = "kms"
key-id = "fixture-key"
region = "fixture-region"
endpoint = "http://kms.invalid"
[metric]
job = "fixture-job"
address = "http://metrics.invalid"
interval = "-1s"
[metering]
type = "s3"
region = "fixture-region"
bucket = "fixture-bucket"
[metering.aws]
s3-force-path-style = true
assume-role-arn = "fixture-role"
''',
'limits':'''[schedule.default-store-limit]
add-peer = 0.0
remove-peer = 2.0
transfer-leader-in = 0.0
[security.encryption]
data-encryption-method = "aes128-ctr"
[security.encryption.master-key]
type = "file"
path = "/fixture/key"
[controller]
push-metrics-address = "http://metrics.invalid"
push-metrics-interval = "3s"
[controller.request-unit]
read-cost-per-byte = -1.0
[metering.localfs]
base-path = "/fixture/metering"
create-dirs = false
permissions = "0700"
'''
}
invalid={
'store-map':'[schedule.store-limit.9]\nadd-peer=3.0\n',
'ru-map':'[controller.ru-version-policy.overrides]\n"42"=3\n',
'space':'[schedule]\nlow-space-ratio=0.2\nhigh-space-ratio=0.8\n',
'worker':'[schedule]\npatrol-region-worker-count=9\n',
'negative-limit':'[schedule.default-store-limit]\nadd-peer=-1.0\n',
'flag-conflict':'[schedule]\ndisable-remove-down-replica=true\nenable-remove-down-replica=true\n',
'isolation':'[replication]\nisolation-level="zone"\n',
'encryption-method':'[security.encryption]\ndata-encryption-method="bad"\n',
'encryption-period':'[security.encryption]\ndata-key-rotation-period="-1s"\n',
'encryption-master':'[security.encryption.master-key]\ntype="bad"\n',
'redact':'[security]\nredact-info-log="invalid"\n',
'log-data-dir':'data-dir="/fixture/data"\n[log.file]\nfilename="/fixture/data/sub/pd.log"\n'
}
fixture={'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'checkout_head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'cases':[]}
for name,body in {**valid,**invalid}.items():
 with tempfile.TemporaryDirectory(prefix='pd97-oracle-') as d:
  data=str(Path(d)/'data')
  prefix='name="nested-oracle"\nclient-urls="http://127.0.0.1:0"\npeer-urls="http://127.0.0.1:0"\n'
  if name!='log-data-dir':prefix+=f'data-dir={json.dumps(data)}\n'
  # PD config log requires json; user-visible log config defaults are tested separately.
  body=prefix+'[log]\nformat="json"\n'+body
  # TOML root fields need to precede [log].
  if name=='log-data-dir':body=prefix+'data-dir="/fixture/data"\n[log]\nformat="json"\n[log.file]\nfilename="/fixture/data/sub/pd.log"\n'
  cfg=Path(d)/'config.toml';cfg.write_text(body)
  record={'name':name,'toml':body}
  if name in invalid:
   result=subprocess.run([str(binary),'--config',str(cfg),'--config-check'],cwd=root,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=20)
   record.update(exit_code=result.returncode,output=result.stdout)
   print(name,result.returncode,result.stdout[:160].replace('\n',' '))
  else:
   proc=subprocess.Popen([str(binary),'--config',str(cfg)],cwd=root,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
   lines=[];deadline=time.monotonic()+20
   try:
    selector=selectors.DefaultSelector();selector.register(proc.stdout,selectors.EVENT_READ)
    while time.monotonic()<deadline:
     if not selector.select(0.25):
      if proc.poll() is not None:break
      continue
     line=proc.stdout.readline()
     if not line:break
     lines.append(line)
     if '"message":"PD config"' in line:
      record['log']=line.rstrip('\n');record['config']=json.loads(line)['config'];break
   finally:
    proc.terminate()
    try:proc.communicate(timeout=10)
    except subprocess.TimeoutExpired:proc.kill();proc.communicate()
   if 'config' not in record:raise RuntimeError(name+''.join(lines))
   record['check_output']=subprocess.check_output([str(binary),'--config',str(cfg),'--config-check'],cwd=root,text=True).strip()
   record['check_messages']=[line for line in record['check_output'].splitlines() if line.startswith(('Config contains undefined item:', 'config check successful'))]
   print(name,'captured',len(record['log']),record['check_messages'])
  fixture['cases'].append(record)
out=root/'server/config/fixtures/nested_config_go.json';out.parent.mkdir(exist_ok=True);out.write_text(json.dumps(fixture,ensure_ascii=False,indent=2)+'\n')
print(out)
