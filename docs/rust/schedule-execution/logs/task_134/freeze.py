"""Source-derived protobuf expectations; never executes Rust or Go."""
import json,base64,pathlib,copy
root=pathlib.Path(__file__).resolve().parents[5]
def var(n):
 n=int(n)&((1<<64)-1); b=bytearray()
 while n>127:b.append((n&127)|128);n>>=7
 return bytes(b)+bytes([n])
def scalar(k,n):return var(k<<3)+var(n) if n else b''
def msg(k,b):return var(k<<3|2)+var(len(b))+b
def opt(k,v,f):return b'' if v is None else msg(k,f(v))
def raw(k,s):
 b=base64.b64decode(s or '');return msg(k,b) if b else b''
def peer(p):return b''.join(scalar(i,p.get(k,0)) for i,k in enumerate(['id','store_id','role','is_witness'],1))
def epoch(p):return scalar(1,p.get('conf_ver',0))+scalar(2,p.get('version',0))
def region(p):return scalar(1,p.get('id',0))+raw(2,p.get('start_key'))+raw(3,p.get('end_key'))+opt(4,p.get('region_epoch'),epoch)+b''.join(msg(5,peer(x)) for x in p.get('peers',[]))+opt(6,p.get('encryption_meta'),lambda e:scalar(1,e.get('key_id',0))+raw(2,e.get('iv')))+scalar(7,p.get('is_in_flashback',False))+scalar(8,p.get('flashback_start_ts',0))
def change(p):return opt(1,p.get('peer'),peer)+scalar(2,p.get('change_type',0))
def transfer(p):return opt(1,p.get('peer'),peer)+b''.join(msg(2,peer(x)) for x in p.get('peers',[]))
def split(p):return scalar(1,p.get('policy',0))+b''.join(msg(2,base64.b64decode(x)) for x in p.get('keys',[]))
names=['change_peer','transfer_leader','merge','split_region','change_peer_v2','switch_witnesses','change_split']
funcs=[change,transfer,lambda p:opt(1,p.get('target'),region),split,lambda p:b''.join(msg(1,change(x)) for x in p.get('changes',[])),lambda p:b''.join(msg(1,scalar(1,x.get('peer_id',0))+scalar(2,x.get('is_witness',False))) for x in p.get('switch_witnesses',[])),lambda p:scalar(1,p.get('auto_split_enabled',False))]
def encode(c,sch):
 fields={1:msg(1,scalar(1,c['cluster_id'])),2 if sch else 4:scalar(2 if sch else 4,c.get('region_id',0)),3 if sch else 5:opt(3 if sch else 5,c.get('region_epoch'),epoch),4 if sch else 6:opt(4 if sch else 6,c.get('target_peer'),peer)}
 for name,f,k in zip(names,funcs,[5,6,7,8,9,10,11] if sch else [2,3,7,8,9,10,11]):fields[k]=opt(k,(c.get('operation') or {}).get(name),f)
 return b''.join(fields[k] for k in sorted(fields)).hex()
cases=[]
for c in json.loads((root/'docs/rust/schedule-execution/fixtures/task_012.json').read_text())['cases']:
 inp={k:copy.deepcopy(c[k]) for k in ['region_id','region_epoch','target_peer','operation']};inp['cluster_id']=123
 cases.append({'id':c['id'],'input':inp})
# Preserve all non-default actions from task 12 while removing unencodable nil entries.
for original in list(cases):
 if original['id'].endswith(('_all','_change_peer_v2','_switch_witnesses')):
  c=copy.deepcopy(original);c['id']+='-encodable'
  for action,key in [('transfer_leader','peers'),('change_peer_v2','changes'),('switch_witnesses','switch_witnesses')]:
   obj=(c['input'].get('operation') or {}).get(action)
   if obj is not None:obj[key]=[x for x in obj.get(key,[]) if x is not None]
  cases.append(c)
# Every independent action combination, including all seven concurrently present.
for mask in range(128):
 cases.append({'id':f'mask-{mask}','input':{'cluster_id':0,'region_id':0,'region_epoch':{},'target_peer':{},'operation':{name:{} for i,name in enumerate(names) if mask&(1<<i)}}})
# Negative/unknown enum values, binary key ordering, present default repeated entries.
for n in [-2147483648,-1,0,1,2,3,2147483647]:
 cases.append({'id':f'enum-{n}','input':{'cluster_id':2**64-1,'region_id':2**64-1,'target_peer':{'role':n},'operation':{'change_peer':{'change_type':n,'peer':{}},'split_region':{'policy':n,'keys':['','AP8=','AP8=']},'transfer_leader':{'peers':[{}, {'id':2},{}]},'change_peer_v2':{'changes':[{},{}]},'switch_witnesses':{'switch_witnesses':[{},{}]}}}})
for path,key in [('transfer_leader','peers'),('change_peer_v2','changes'),('switch_witnesses','switch_witnesses')]:
 for idx in [0,1,2]:
  xs=[{}, {}, {}];xs[idx]=None
  cases.append({'id':f'nil-{path}-{idx}','input':{'cluster_id':123,'target_peer':{},'operation':{path:{key:xs}}}})
for c in cases:
 inp=c['input']
 if inp.get('target_peer') is None:c['expected']='suppressed';continue
 op=inp.get('operation') or {}
 for action,key in [('transfer_leader','peers'),('change_peer_v2','changes'),('switch_witnesses','switch_witnesses')]:
  xs=(op.get(action) or {}).get(key,[])
  if None in xs:c.update(error=action+'.'+key,index=xs.index(None));break
 if 'error' not in c:c.update(pdpb_hex=encode(inp,False),schedulingpb_hex=encode(inp,True))
out=root/'docs/rust/schedule-execution/fixtures/task_134.json'
out.write_text(json.dumps({'evidence_kind':'source-derived, NOT Go runtime output','normalization':'none; nil repeated messages explicitly rejected (Go generated marshaler dereferences nil)','proto_commit':'f8ad079c8e9812ed5900b0c12b5f1d97171464c4','cases':cases},indent=2)+'\n')
print(len(cases),'cases',sum('error'in c for c in cases),'errors',sum(c.get('expected')=='suppressed' for c in cases),'suppressed')
