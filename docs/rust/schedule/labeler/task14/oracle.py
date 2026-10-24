"""Existing read-only Go binary, isolated container; no Go build or injection."""
import copy, hashlib, json, pathlib, subprocess, time, urllib.request, urllib.error
import grpc
root=pathlib.Path(__file__).parent
port=subprocess.check_output(['docker','port','pd-task14-oracle','2379/tcp'],text=True).strip().rsplit(':',1)[1]
base='http://127.0.0.1:'+port

def http(path, value=None):
    req=urllib.request.Request(base+path, data=None if value is None else json.dumps(value).encode(),headers={'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(req,timeout=10) as r:return r.status,json.load(r)
    except urllib.error.HTTPError as e:return e.code,json.load(e)
for _ in range(80):
    try:cluster=http('/pd/api/v1/cluster')[1]['id'];break
    except Exception:time.sleep(.25)
else:raise RuntimeError('not ready')
def varint(v):
    b=bytearray()
    while v>127:b.append((v&127)|128);v>>=7
    return bytes(b)+bytes([v])
def integer(n,v):return varint(n*8)+varint(v)
def blob(n,v):return varint(n*8+2)+varint(len(v))+v
header=integer(1,cluster)
store=integer(1,1)+blob(2,b'127.0.0.1:20160')
region=integer(1,2)+blob(4,integer(1,1)+integer(2,1))+blob(5,integer(1,3)+integer(2,1))
with grpc.insecure_channel('127.0.0.1:'+port) as channel:
    result=channel.unary_unary('/pdpb.PD/Bootstrap')(blob(1,header)+blob(2,store)+blob(3,region),timeout=10)
    print('bootstrap response',result.hex())
base_rule={'id':'r','index':-2,'labels':[{'key':'k','value':'v'}],'rule_type':'key-range','data':[{'start_key':'AA','end_key':''}]}
cases=[]
def add(name, mutate):
    r=copy.deepcopy(base_rule);mutate(r);cases.append({'name':name,'input':r})
add('valid',lambda r:None)
add('empty_id',lambda r:r.update(id=''))
add('empty_labels',lambda r:r.update(labels=[]))
add('empty_key',lambda r:r['labels'][0].update(key=''))
add('empty_value',lambda r:r['labels'][0].update(value=''))
add('invalid_type',lambda r:r.update(rule_type='unknown'))
for name,data in [('object',{}),('null',None),('empty',[]),('item_number',[42]),('missing_start',[{'end_key':''}]),('missing_end',[{'start_key':''}]),('numeric_start',[{'start_key':1,'end_key':''}]),('odd_hex',[{'start_key':'a','end_key':''}]),('bad_hex',[{'start_key':'xx','end_key':''}]),('bad_end',[{'start_key':'','end_key':'xy'}]),('equal',[{'start_key':'aa','end_key':'AA'}]),('reverse',[{'start_key':'ff','end_key':'aa'}]),('bounded',[{'start_key':'00','end_key':'01'}]),('multiple',[{'start_key':'AA','end_key':''},{'start_key':'','end_key':'00'}])]:
    add(name,lambda r,d=data:r.update(data=d))
for name,ttl,start in [('invalid_ttl','ttl',''),('overflow_ttl','9223372036854775808ns',''),('invalid_start','1h','bad'),('expired','1h','Mon Jan  2 15:04:05 UTC 2006'),('negative','-1h','Mon Jan  2 15:04:05 UTC 2006'),('future','10h10m10s10ms','Fri Jan  1 00:00:00 UTC 2100'),('zone','1h','Fri Jan  1 00:00:00 XYZ 2100'),('fraction','1h','Fri Jan  1 00:00:00.123456789 UTC 2100'),('auto','1h',''),('no_ttl','','bad')]:
    add(name,lambda r,t=ttl,s=start:r['labels'][0].update(ttl=t,start_at=s))
add('partial_expired',lambda r:r['labels'].extend([{'key':'old','value':'v','ttl':'1h','start_at':'Mon Jan  2 15:04:05 UTC 2006'},{'key':'future','value':'v','ttl':'1h','start_at':'Fri Jan  1 00:00:00 UTC 2100'}]))
add('null_label',lambda r:r.update(labels=[None]))
add('null_scalar',lambda r:r['labels'][0].update(ttl=None,start_at=None))
add('case_insensitive',lambda r:(r.update(ID=r.pop('id'),LABELS=r.pop('labels'))))
for name,start in [('gmt_offset','Fri Jan  1 00:00:00 GMT+8 2100'),('signed_zone','Fri Jan  1 00:00:00 +03 2100'),('mixed_zone','Fri Jan  1 00:00:00 ChST 2100'),('invalid_zone','Fri Jan  1 00:00:00 ABCDE 2100'),('invalid_day','Fri Feb 30 00:00:00 UTC 2100'),('weekday_ignored','Mon Jan  1 00:00:00 UTC 2100')]:
    add(name,lambda r,s=start:r['labels'][0].update(ttl='1h',start_at=s))
for case in cases:
    status,out=http('/pd/api/v1/config/region-label/rule',case['input']);case.update(status=status,output=out)
    if status==200:
        status,rule=http('/pd/api/v1/config/region-label/rule/r');assert status==200
        case['adjusted']=rule
    print(case['name'],case['status'],case['output'])
(root/'go.json').write_text(json.dumps({'sha256':hashlib.sha256(pathlib.Path('/tmp/pd-types-oracle/pd-classic-linux').read_bytes()).hexdigest(),'cases':cases},indent=2)+'\n')
