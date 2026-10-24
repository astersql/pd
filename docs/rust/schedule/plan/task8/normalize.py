"""Normalize only Go map order, debugger addresses and named-vs-numeric codes."""
import json, pathlib, re
root=pathlib.Path(__file__).parent
out={}
for segment in (root/'go.out').read_text().split('SCENARIO ')[1:]:
    name=segment.splitlines()[0]
    summary=segment.rsplit('Values returned:',1)[1]
    statuses={int(i):int(c or named) for i,c,named in re.findall(r'(\d+): \{StatusCode: (?:(-?\d+)|\w+ \((-?\d+)\))',summary)}
    assert len(statuses)==len(re.findall('DetailedReason:',summary))
    assert all(reason=='' for reason in re.findall(r'DetailedReason: "([^"]*)"',summary))
    out[name]={'statuses':statuses,'normal':re.search(r'~r1: (true|false)',summary)[1]=='true',
      'error':None if '~r2: error nil' in summary else 'PD:diagnostic:ErrDiagnosticLoadPlan'}
expected={
 'analyzer1':([102]*5,True), 'analyzer2':([403]*5,False),
 'analyzer3':([1004,1004,1004,1004,403],False),
 'analyzer4':([101,101,102,102,403],False), 'analyzer5':([101,101,102,102,203],False),
 'analyzer6':([403]*5,False), 'empty':([],True), 'success':([0],False),
 'majority':([403],False), 'tie':([400],False), 'priority':([102],True),
 'skip_lower':([102],True), 'replace_lower':([102],False),
}
for name,(codes,flag) in expected.items():
    assert out[name]==dict(statuses=dict(enumerate(codes,1)),normal=flag,error=None),(name,out[name])
assert out['invalid_plan']==dict(statuses={},normal=False,error='PD:diagnostic:ErrDiagnosticLoadPlan')
(root/'normalized.json').write_text(json.dumps(out,indent=2)+'\n')
print('14 Go output scenarios verified (13 success + 1 typed error)')
