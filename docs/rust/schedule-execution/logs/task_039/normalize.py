"""Remove Git whitespace violations while retaining reversible raw-byte evidence."""
from pathlib import Path
import hashlib,json,base64
out=Path(__file__).resolve().parent
records=[]
for p in sorted(out.glob('*.log')):
 raw=p.read_bytes()
 lines=raw.splitlines(keepends=True)
 clean=(b''.join(line.rstrip(b'\r\n').rstrip(b' \t')+b'\n' for line in lines).rstrip(b'\n')+b'\n') if raw else b''
 if raw!=clean:
  # Store the reversible original only when it differs (normally trailing lines).
  diffs=[]
  for i,(a,b) in enumerate(zip(raw.splitlines(keepends=True),clean.splitlines(keepends=True))):
   if a!=b:diffs.append(dict(line=i+1,original_base64=base64.b64encode(a).decode()))
  extra=raw.splitlines(keepends=True)[len(clean.splitlines(keepends=True)):]
  records.append(dict(file=p.name,raw_sha256=hashlib.sha256(raw).hexdigest(),normalized_sha256=hashlib.sha256(clean).hexdigest(),changed_lines=diffs,trailing_original_base64=base64.b64encode(b''.join(extra)).decode()))
  p.write_bytes(clean)
(out/'log-normalization.json').write_text(json.dumps(records,indent=2)+'\n')
