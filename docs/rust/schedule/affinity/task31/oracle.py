"""Read-only preexisting Go binary in an isolated container; no Go build."""
import pathlib,json,hashlib,urllib.parse
root=pathlib.Path(__file__).parent
g={'__file__':str(root.parents[1]/'labeler/task14/oracle.py')}
exec((root.parents[1]/'labeler/task14/oracle.py').read_text().split('cases=[]')[0].replace('pd-task14-oracle','pd-task31-oracle'),g)
http=g['http'];out={'sha256':hashlib.sha256(pathlib.Path('/tmp/pd-types-oracle/pd-classic-linux').read_bytes()).hexdigest(),'cases':[]}
p='/pd/api/v2/affinity-groups'
status,response=http(p,{'affinity_groups':{'a':{'ranges':[{'start_key':'YQ==','end_key':'Yg=='}]},'B_1-':{'ranges':[{'start_key':'Yg==','end_key':'Yw=='}]}}})
print('CREATE',status,response,flush=True)
assert status==200
out['created']=response
for raw in [[],[' a ','a','','\u0085B_1-\u3000'],[' '],['a','missing'],['bad/id'],['a\n'],['a'*64],['a'*65],['é']]:
 status,response=http(p+('?' + urllib.parse.urlencode([('ids',v) for v in raw]) if raw else ''))
 out['cases'].append(dict(raw=raw,status=status,response=response));print(raw,status,response,flush=True)
(root/'go.json').write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n')
